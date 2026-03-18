//! Botanix Reth node entry point.
//!
//! This crate provides the main entry point for running a Botanix Reth node
//! with Botanix support.

use botanix_authority_peg::mint_validation::MINT_CONTRACT_ADDRESS;
use botanix_authority_rsp::RandomSourceProvider;
use botanix_btc_server_client::BtcServerExtendedClient;
use botanix_btc_wallet::fallback::ClientSelection;
use botanix_chainspec::{
    constants::{BOTANIX_MAINNET_CHAIN_ID, BOTANIX_TESTNET_CHAIN_ID},
    parser::BotanixChainSpecParser,
};
use botanix_cli_args::{
    chain::{get_botanix_chain_from_federation_config, BotanixNetwork},
    BotanixArgs,
};
use botanix_configs::federation::{
    load_federation_config_toml, AuthorityMultisigConfig,
};
use botanix_reth::{
    consensus::{
        comet_bft::abci::ABCIDriver,
        multisig_manager::{attested_legacy_multisig, MultisigManager},
        snapshot_manager::SnapshotRunnable,
        utils::{is_known_minting_contract, retry_exec},
        AuthorityConsensusBuilder, OperatorBuilder,
    },
    node::{
        consensus::BotanixConsensus, evm::config::BotanixEvmConfig,
        storage::BotanixStorage, BotanixNode,
    },
    services::{
        activation_manager::setup_activation_manager,
        bitcoin_checkpoints::setup_bitcoin_checkpoints,
        bitcoind::setup_bitcoind_client,
        botanix_provider::create_botanix_provider,
        btc_server::create_btc_server_client,
        cometbft::create_cometbft_factory,
        frost::setup_frost,
        metrics::run_metrics_service,
        migrator::init_and_migrate_botanix_db,
        network_builder::{lookup_head, setup_network_builder},
        provider::create_blockchain_provider,
        recover_utxos::recover_missing_utxos,
        reth::{load_reth_config, verify_federation_config_hash},
        rpc::rpc::setup_and_run_rpc,
    },
};
use botanix_storage::{tables::create_botanix_tables, BotanixProviderFactory};
use botanix_utils::panic_hook::set_panic_hook;
use clap::Parser;
use reth::{
    cli::{Cli, Commands},
    providers::CanonStateSubscriptions,
};
use reth_db::DatabaseEnv;
use reth_node_builder::RethTransactionPoolConfig;
use reth_node_core::version::version_metadata;
use reth_prune_types::PruneModes;
use reth_transaction_pool::{
    blobstore::InMemoryBlobStore, TransactionValidationTaskExecutor,
};
use std::{sync::Arc, time::Duration};
use tracing::{debug, error, info};

// We use jemalloc for performance reasons
#[cfg(all(feature = "jemalloc", unix))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn main() -> eyre::Result<()> {
    reth_cli_util::sigsegv_handler::install();

    tracing::info!(target: "reth::cli", version = ?version_metadata().short_version, "Starting reth with poa");
    set_panic_hook();

    // Enable backtraces unless a RUST_BACKTRACE value has already been
    // explicitly provided.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        unsafe {
            std::env::set_var("RUST_BACKTRACE", "full");
        }
    }
    // Parse everything first
    let cli = Cli::<BotanixChainSpecParser, BotanixArgs>::parse();

    // Pull out the Node subcommands.
    let (
        network_args,
        original_rpc_server_args,
        datadir_args,
        db_args,
        metrics_args,
        txpool_args,
    ) = match &cli.command {
        Commands::Node(cmd) => {
            let node_cmd = cmd.as_ref();
            (
                node_cmd.network.clone(),
                node_cmd.rpc.clone(),
                node_cmd.datadir.clone(),
                node_cmd.db.clone(),
                node_cmd.metrics.clone(),
                node_cmd.txpool.clone(),
            )
        }
        _ => {
            panic!("Only 'node' command is supported in Botanix Reth");
        }
    };

    cli.run_with_components::<BotanixNode>(
    |spec| (BotanixEvmConfig::new(spec.clone()), BotanixConsensus::new(spec)),
        async move |builder, args: BotanixArgs| {
            tracing::info!(
                target: "botanix",
                "p2p addr={} port={} trusted_only={}",
                network_args.addr, network_args.port, network_args.trusted_only
            );

            // Bitcoind Config
            let bitcoind_cfg = args.bitcoind.clone();

            // POA Config
            let poa_cfg = args.poa.clone();

            verify_federation_config_hash(&poa_cfg)?;

            // State Sync Config
            let state_sync_cfg = args.poa.state_sync.clone();

            // Reth Config
            let mut reth_cfg = load_reth_config(&args.poa, &network_args)?;

            // Tx Pool Config
            let tx_pool_config = txpool_args.pool_config();

            // Testnet and Devnet should result in the same chain spec
            let botanix_network = BotanixNetwork::from_args(poa_cfg.is_testnet, poa_cfg.is_devnet)?;

            let federation_toml = load_federation_config_toml(
                poa_cfg
                    .federation_config_path
                    .clone()
                    .to_str()
                    .expect("federation config path to exist"),
            )?;

            // TODO: This should reuse `botanix_fee_recipient`
            let chain_spec = get_botanix_chain_from_federation_config(
                federation_toml.clone(),
                !botanix_network.is_mainnet(),
            )?;
            let chain_spec_arc = Arc::new(chain_spec.clone());

            // Check chains match
            match (chain_spec_arc.inner().chain.id(), bitcoind_cfg.btc_network) {
                (BOTANIX_MAINNET_CHAIN_ID, bitcoin::Network::Bitcoin) => {}
                (BOTANIX_TESTNET_CHAIN_ID, _) => {
                    // Testnet can be any non-mainnet network for btc
                    if bitcoind_cfg.btc_network == bitcoin::Network::Bitcoin {
                        return Err(eyre::eyre!(
                            "Chains mismatch: Botanix is testnet and btc network is not."
                        ));
                    }
                }
                _ => {
                    return Err(eyre::eyre!(
                        "Chains mismatch: Botanix is mainnet and btc network is not."
                    ));
                }
            }

            // Create bitcoind client
            let bitcoind_client = setup_bitcoind_client(&bitcoind_cfg, ClientSelection::Fallback)?;
            let bitcoind_client = Arc::new(bitcoind_client);

            // Setup the activation manager
            let activation_manager = setup_activation_manager(&botanix_network);

            // Create frost manager
            let frost_setup_result = setup_frost(
                &chain_spec,
                &datadir_args,
                federation_toml.clone(),
                &network_args,
                &mut reth_cfg,
            )?;

            let botanix_provider = create_botanix_provider(&bitcoind_cfg, bitcoind_client.clone())?;

            // Setup bitcoin checkpoints synchronizer
            let (checkpoints_synchronizer, bitcoin_zmq_block_hash_stream, bitcoin_checkpoints) = setup_bitcoin_checkpoints(
                bitcoind_client.clone(),
                &bitcoind_cfg,
                &chain_spec,
            ).await?;

            let reth_database: Arc<DatabaseEnv> = builder.db().clone();
            // Migrate the db if needed
            let mut botanix_database = init_and_migrate_botanix_db(
                reth_database.clone(),
                &datadir_args,
                Arc::clone(&chain_spec_arc),
                &db_args
            )?;

            // For the initial chain startup, the custom Botanix tables need to be added
            if let Some(db) = Arc::get_mut(&mut botanix_database) {
                create_botanix_tables(db)
                    .map_err(|e| eyre::eyre!("Failed to create Botanix tables: {}", e))?;
                info!("Successfully created Botanix tables");
            } else {
                debug!("Skipping Botanix table creation: database already initialized");
            }

            // Create a blockchain provider
            let (blockchain_provider, static_files_provider, reth_provider_factory) = create_blockchain_provider(
                chain_spec_arc.clone(),
                &datadir_args,
                reth_database.clone()
            )?;

            // Get the task executor
            let task_executor = builder.task_executor().clone();

            // Create a tx pool
            let blob_store = InMemoryBlobStore::default();
            let head = lookup_head(&blockchain_provider)?;
            let validator = TransactionValidationTaskExecutor::eth_builder(blockchain_provider.clone())
            .with_head_timestamp(head.timestamp)
            .with_minimum_priority_fee(tx_pool_config.minimum_priority_fee)
            .with_additional_tasks(1)
            .build_with_tasks(task_executor.clone(), blob_store.clone());

            let pool = reth_transaction_pool::Pool::eth_pool(validator, blob_store, tx_pool_config);
            info!(target: "reth::cli", "Transaction pool initialized");

            // spawn txpool maintenance task
            {
                let pool = pool.clone();
                let chain_events = blockchain_provider.canonical_state_stream();
                task_executor.spawn_critical(
                    "txpool maintenance task",
                    reth_transaction_pool::maintain::maintain_transaction_pool_future(
                        blockchain_provider.clone(),
                        pool,
                        chain_events,
                        task_executor.clone(),
                        Default::default(),
                    ),
                );
                debug!(target: "reth::cli", "Spawned txpool maintenance task");
            }

            let state_provider = reth_provider_factory.latest().expect("provider factory to exist");
            let deployed_bytecode = state_provider
                .account_code(&*MINT_CONTRACT_ADDRESS)
                .expect("Minting contract address exists")
                .expect("Minting contract bytecode to exist");

            if let Err(e) = is_known_minting_contract(
                federation_toml.minting_contract_bytecode.clone(),
                &deployed_bytecode.original_bytes(),
            ) {
                error!(target: "reth::cli", "{}", e);
                panic!("{}", e);
            }

            // Create provider factories
            let storage = Arc::new(BotanixStorage::default());
            let reth_db_provider_factory = BotanixProviderFactory::<Arc<DatabaseEnv>, BotanixNode>::new(reth_database.clone(), chain_spec_arc.clone(), static_files_provider.clone(), PruneModes::none(), storage.clone());
            let botanix_db_provider_factory = BotanixProviderFactory::<Arc<DatabaseEnv>, BotanixNode>::new(botanix_database.clone(), chain_spec_arc.clone(), static_files_provider.clone(), PruneModes::none(), storage.clone());

            // Create and connect to btc signining server if in federation mode
            let mut btc_server_client = create_btc_server_client(&poa_cfg, &bitcoind_cfg).await?;

            // Eventually we want to recover missing UTXOs on every start
            if let Some((_, ref mut btc_server_client)) = btc_server_client.as_mut() {
                recover_missing_utxos(&poa_cfg, btc_server_client).await?;
            }

            let (network_handle, network_manager, tx_pool_p2p, eth_request_handler_p2p, frost_p2p) =
            setup_network_builder(
                &frost_setup_result,
                &reth_provider_factory,
                &blockchain_provider,
                &reth_cfg,
                &chain_spec_arc.inner_arc(),
                &poa_cfg,
                &network_args,
                &datadir_args,
                task_executor.clone(),
                pool.clone(),
            ).await?;

            // Start frost p2p tasks (only available for federation nodes)
            let frost_handle = if poa_cfg.federation_mode {
                let frost_manager = frost_p2p.expect("frost p2p must exist in federation mode");
                let handle = frost_manager.handle();
                task_executor.spawn_critical("p2p frost", frost_manager);
                Some(handle)
            } else {
                None
            };

            let legacy_multisig = if poa_cfg.is_testnet {
                None
            } else if poa_cfg.is_devnet {
                None
            } else {
                Some(attested_legacy_multisig())
            };

            // Setup multisig manager with optionally preloaded legacy multisig.
            let (multisig_manager, multisig_handle) = MultisigManager::new_botanix(botanix_db_provider_factory.clone(), legacy_multisig)?;

            for m in frost_setup_result.multisigs.clone() {
                // TODO: This could be a problem for expired multisigs, since it
                // might reinsert those on startup.
                multisig_manager.guard_commit(|g| {
                    g.set_staging_multisig(
                        m.multisig_id,
                        m.coordinator,
                        m.authorities,
                    )
                })?;
            }

            let botanix_evm_config = BotanixEvmConfig::new(chain_spec_arc.clone());
            let cometbft_rpc_factory = create_cometbft_factory(&poa_cfg);
            let btc_server_factory = btc_server_client.unzip().0;
            let (driver_tx, driver_rx) = tokio::sync::mpsc::channel(1);

            let (abci_client_builder, snapshot_manager, consensus) = match AuthorityConsensusBuilder::try_new(
                chain_spec_arc.clone(),
                blockchain_provider.clone(),
                activation_manager,
                multisig_manager.clone(),
                poa_cfg.federation_mode, // is_fed_node
                bitcoin_checkpoints.clone(),
                task_executor.clone(),
                bitcoind_cfg.btc_network,
                botanix_evm_config,
                cometbft_rpc_factory.clone(),
                driver_tx,
                state_sync_cfg.clone(),
                reth_provider_factory.clone(),
                botanix_db_provider_factory.clone(),
                poa_cfg.block_fee_recipient_address,
                bitcoind_client.clone(),
            ) {
                std::result::Result::Ok(consensus) => consensus.build::<BtcServerExtendedClient>().await,
                std::result::Result::Err(e) => {
                    return Err(eyre::eyre!("AuthorityConsensusBuilderError : {:?}", e));
                }
            };

            let storage = if let Some(abci_client_builder) = abci_client_builder.as_ref() {
                abci_client_builder.storage().clone()
            } else {
                panic!("ABCI client builder should exist in authority mode");
            };

            let mut abci_driver = ABCIDriver::new(
                driver_rx,
                reth_db_provider_factory.clone(),
                botanix_db_provider_factory,
                blockchain_provider.clone(),
                multisig_manager,
            );

            if poa_cfg.federation_mode {
                let Some(btc_server_factory) = btc_server_factory.clone() else {
                    return Err(eyre::eyre!("btc-server mut be configured for authority"));
                };

                // TODO: Just filter non-membership configs?
                let multisig_configs = frost_setup_result
                    .multisigs
                    .into_iter()
                    .map(|m| AuthorityMultisigConfig::try_from(m).map_err(Into::into))
                    .collect::<eyre::Result<Vec<_>>>()?;

                let operator = OperatorBuilder::new(
                    storage,
                    btc_server_factory,
                    network_handle.clone(),
                    frost_handle.expect("frost handle must exist in federation mode"),
                    multisig_handle,
                    task_executor.clone(),
                    multisig_configs,
                    cometbft_rpc_factory,
                    RandomSourceProvider::new(),
                    bitcoind_client,
                );

                let mut frost_task = operator.build::<BtcServerExtendedClient>().await;

                // TODO: Should be spawned by OperatorBuilder directly?
                task_executor.spawn_critical(
                    "FrostTask", // TODO: Should be labeled
                    Box::pin(async move {
                        if let Err(e) = frost_task.start_task().await {
                            tracing::error!(target: "reth::cli", "Frost Task Error: {:?}", e);
                        }
                    }),
                );
            }

            // Setup and launch RPC server
            let _rpc_handle = setup_and_run_rpc(
                blockchain_provider.clone(),
                &original_rpc_server_args,
                &task_executor,
                Arc::clone(&chain_spec_arc),
                botanix_provider.clone(),
                pool.clone(),
                network_handle.clone(),
                consensus,
            ).await?;

            // launch the network manager task
            task_executor.spawn_critical("network p2p", network_manager);
            task_executor.spawn_critical("txpool p2p task", tx_pool_p2p);
            task_executor.spawn_critical("eth request handler p2p task", eth_request_handler_p2p);

            // NOTE: the node will block here until DKG has completed
            let abci_client_builder = abci_client_builder.expect("abci client builder exists");
            let fut = || async {
                abci_client_builder
                    .start_server(
                        &task_executor.clone(),
                        pool.clone(),
                        poa_cfg.abci_host.to_string(),
                        poa_cfg.abci_port,
                    )
                    .await
            };

            match retry_exec("abci_server_start", fut, 3, Duration::from_secs(2)).await {
                std::result::Result::Ok(()) => {}
                std::result::Result::Err(err) => {
                    tracing::error!(target: "reth::cli", "Failed to connect to abci client: {}", err);
                    return Err(eyre::eyre!("Failed to connect to abci client: {}", err));
                }
            };

            if let Some(mut snapshot_manager) = snapshot_manager {
                tracing::info!("Snapshot manager is enabled.");
                task_executor.spawn_critical(
                    "Snapshot Manager",
                    Box::pin(async move {
                        if let Err(e) = snapshot_manager.run().await {
                            tracing::error!(target: "reth::cli", "Snapshot Manager Error: {:?}", e);
                        }
                    }),
                );
            }

            // add metrics if necessary
            run_metrics_service(metrics_args, &task_executor, chain_spec_arc).await?;

            // launch the bitcoin checkpoints synchronizer task
            task_executor.spawn_critical(
                "async bitcoin checkpoint chain synchronization task",
                checkpoints_synchronizer.sync(bitcoin_zmq_block_hash_stream),
            );
            tracing::info!(target: "reth::cli", "Spawned async bitcoin task for block headers");

            let (tx, rx) = tokio::sync::oneshot::channel();
            task_executor.spawn_critical(
                "abci driver",
                Box::pin(async move {
                    let res = abci_driver.start().await;
                    let _ = tx.send(res);
                }),
            );

            match rx.await? {
                std::result::Result::Ok(()) => tracing::info!(target: "reth::cli", "ABCIDriver exited successfully"),
                std::result::Result::Err(error) => {
                    tracing::error!(target: "reth::cli", %error, "ABCIDriver exited with an error")
                }
            };

            // Launch the reth stages metrics listener task
            tracing::debug!(target: "reth::cli", "Spawning stages metrics listener task");
            let (_sync_metrics_tx, sync_metrics_rx) = tokio::sync::mpsc::unbounded_channel();
            let sync_metrics_listener = reth_stages::MetricsListener::new(sync_metrics_rx);
            task_executor.spawn_critical("stages metrics listener task", sync_metrics_listener);

            // Block and wait for node termination
            Ok(())
        },
    )?;

    Ok(())
}
