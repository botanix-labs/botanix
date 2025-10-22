//! Botanix Reth node entry point.
//!
//! This crate provides the main entry point for running a Botanix Reth node with Botanix support.

use std::{sync::Arc, time::Duration};
use botanix_authority_rsp::RandomSourceProvider;
use botanix_btc_server_client::BtcServerExtendedClient;
use botanix_chainspec::{constants::{BOTANIX_MAINNET_CHAIN_ID, BOTANIX_TESTNET_CHAIN_ID}, parser::BotanixChainSpecParser};
use botanix_cli_args::{chain::{get_chain_from_federation_config, BotanixNetwork}, BotanixArgs};
use botanix_storage::BotanixProviderFactory;
use botanix_utils::panic_hook::set_panic_hook;
use btcserverlib::version::{CARGO_PKG_VERSION, VERGEN_BUILD_TIMESTAMP, VERGEN_GIT_SHA};
use clap::Parser;
use eyre::Ok;
use reth::cli::{Cli, Commands};
use reth_botanix::{
    botanix_authority_consensus::{comet_bft::abci::ABCIDriver, snapshot_manager::SnapshotRunnable, utils::retry_exec, wallet_state_sync::WalletStateSync, AuthorityConsensusBuilder}, node::{consensus::BotanixConsensus, evm::config::BotanixEvmConfig, BotanixNode}, services::{activation_manager::setup_activation_manager, bitcoin_checkpoints::setup_bitcoin_checkpoints, bitcoind::setup_bitcoind_client, botanix_provider::create_botanix_provider, btc_server::create_btc_server_client, cometbft::create_cometbft_factory, frost::setup_frost, migrator::init_and_migrate_db, provider::create_blockchain_provider, recover_utxos::recover_missing_utxos, reth::load_reth_config, rpc::setup_and_run_rpc},
};
use reth::providers::HeaderProvider;
use reth_cli_commands::NodeCommand;
use reth_db::DatabaseEnv;
use reth_network::{NetworkConfigBuilder, NetworkManager};
use reth_node_builder::NodeTypesWithDBAdapter;
use reth_node_core::version::version_metadata;
use reth_node_metrics::{chain::ChainSpecInfo, hooks::Hooks, server::{MetricServer, MetricServerConfig}, version::VersionInfo};
use reth_provider::{providers::BlockchainProvider, BlockHashReader, StageCheckpointReader};
use reth_stages::StageId;
use alloy_eip2124::Head;

// We use jemalloc for performance reasons
#[cfg(all(feature = "jemalloc", unix))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn main() -> eyre::Result<()> {
    reth_cli_util::sigsegv_handler::install();

    tracing::info!(target: "reth::cli", version = ?version_metadata().short_version, "Starting reth with poa");
    set_panic_hook();

    // Enable backtraces unless a RUST_BACKTRACE value has already been explicitly provided.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    // Parse everything first
    let cli = Cli::<BotanixChainSpecParser, BotanixArgs>::parse();

    // Pull out the Node subcommand so we can access its fields like `network`.
    let node_cmd: &NodeCommand<BotanixChainSpecParser, BotanixArgs> = match &cli.command {
        Commands::Node(cmd) => cmd.as_ref(),
        // If the user ran a non-node command (e.g. `reth db`), just execute it normally.
        _ => {
            // fall back to running without custom launcher
            cli.run_with_components::<BotanixNode>(
                |spec| (BotanixEvmConfig::new(spec.clone()), BotanixConsensus::new(spec)),
                |_builder, _args| async { Ok(()) },
            )?;
            std::process::exit(0);
        }
    };
    let network_args = node_cmd.network.clone();
    let rpc_server_args = node_cmd.rpc.clone();
    let datadir_args = node_cmd.datadir.clone();
    let db_args = node_cmd.db.clone();
    let metrics_args = node_cmd.metrics.clone();

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

            // Frost Config
            let frost_cfg = args.frost.clone();

            // POA Config
            let poa_cfg = args.poa.clone();

            // State Sync Config
            let state_sync_cfg = args.state_sync.clone();

            // Reth Config
            let mut reth_cfg = load_reth_config(&args.poa, &network_args)?;

            // Testnet and Devnet should result in the same chain spec
            let botanix_network = BotanixNetwork::from_args(poa_cfg.is_testnet, poa_cfg.is_devnet)?;
            let chain_spec = get_chain_from_federation_config(
                poa_cfg
                    .federation_config_path
                    .clone()
                    .to_str()
                    .expect("federation config path to exist"),
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
            let (bitcoind_client, bitcoind_client_factory) = setup_bitcoind_client(&bitcoind_cfg, &poa_cfg).await?;

            // Migrate the db if needed
            let (reth_database, botanix_database) = init_and_migrate_db(
                &datadir_args,
                Arc::clone(&chain_spec_arc),
                &db_args
            )?;

            let reth_db_provider_factory = BotanixProviderFactory::<Arc<DatabaseEnv>>::new(reth_database.clone());
            let botanix_db_provider_factory = BotanixProviderFactory::<Arc<DatabaseEnv>>::new(botanix_database.clone());

            // Create a blockchain provider
            let (blockchain_provider, _static_files_provider, reth_provider_factory) = create_blockchain_provider(
                chain_spec_arc.clone(),
                &datadir_args,
                reth_database.clone()
            )?;

            // Create and connect to btc signining server if in federation mode
            let mut btc_server_client = create_btc_server_client(&poa_cfg, &bitcoind_cfg).await?;

            // Eventually we want to recover missing UTXOs on every start
            if let Some((_, ref mut btc_server_client)) = btc_server_client.as_mut() {
                recover_missing_utxos(&poa_cfg, btc_server_client).await?;
            }

            // Setup the activation manager
            let activation_manager = setup_activation_manager(&botanix_network);

            // Create frost manager
            let frost_setup_result = setup_frost(
                &chain_spec,
                &datadir_args,
                &poa_cfg,
                &network_args,
                &frost_cfg,
                &state_sync_cfg,
                &mut reth_cfg,
            )?;

            let botanix_provider = create_botanix_provider(&bitcoind_cfg, &bitcoind_client_factory)?;

            // Setup bitcoin checkpoints synchronizer
            let (checkpoints_synchronizer, bitcoin_zmq_block_hash_stream, bitcoin_checkpoints) = setup_bitcoin_checkpoints(
                bitcoind_client,
                &bitcoind_cfg,
                &chain_spec,
            ).await?;

            // build the node
            let (node, engine_handle_tx) = BotanixNode::new();

            // launch the node
            let reth::builder::NodeHandle { node, node_exit_future } =
                builder.node(node).launch().await?;

            // Setup and launch RPC server
            setup_and_run_rpc(
                blockchain_provider.clone(),
                &rpc_server_args,
                &node.task_executor,
                Arc::clone(&chain_spec_arc),
                botanix_provider.clone(),
            ).await?;


            // ================================================
            let secret_key = frost_setup_result.secret_key.clone();
            let data_dir = datadir_args.datadir.unwrap_or_chain_default(chain_spec_arc.chain, datadir_args.clone());
            let default_peers_path = data_dir.known_peers();
            let head = lookup_head(&blockchain_provider);

            let mut network_cfg_builder: NetworkConfigBuilder = network_args
                .network_config(&reth_cfg, chain_spec_arc.clone(), secret_key, default_peers_path)
                .with_task_executor(Box::new(node.task_executor.clone()))
                .set_head(head)
                .listener_addr(std::net::SocketAddr::new(
                    network_args.addr,
                    network_args.port,
                ))
                .discovery_addr(std::net::SocketAddr::new(
                    network_args.addr,
                    network_args.port,
                ));

            // Optionally disable discovery if needed
            if network_args.trusted_only {
                network_cfg_builder = network_cfg_builder.disable_discovery();
            }

            // Set network mode to Authority if this is a validator/authority node
            if poa_cfg.federation_mode {
                network_cfg_builder = network_cfg_builder
                    .network_mode(reth_network::config::NetworkMode::Authority);
            }
            let network_config = network_cfg_builder.build(reth_provider_factory.clone());

            // Create the network manager and get the handle
            let (network_handle, network_manager, tx_pool_p2p, eth_request_handler_p2p, frost_p2p) =
                NetworkManager::builder(network_config)
                    .await?
                    .frost(frost_setup_result.frost_config.clone())
                    .request_handler(reth_provider_factory.clone())
                    .transactions(node.pool.clone(), Default::default())
                    .split_with_handle();

            // Start all the p2p tasks
            let frost_handle = if poa_cfg.federation_mode {
                let frost_manager = frost_p2p.expect("should be some");
                let frost_handle = frost_manager.handle();
                node.task_executor.spawn_critical("p2p frost", frost_manager);
                Some(frost_handle)
            } else {
                None
            };

            // ================================================================

            let (driver_tx, driver_rx) = tokio::sync::mpsc::channel(1);
            let mut abci_driver = ABCIDriver::new(
                driver_rx,
                reth_db_provider_factory.clone(),
                botanix_db_provider_factory.clone(),
                blockchain_provider.clone(),
            );

            let task_executor = node.task_executor.clone();
            let botanix_evm_config = BotanixEvmConfig::new(chain_spec_arc.clone());
            let (bitcoind_client, bitcoind_factory) = setup_bitcoind_client(&bitcoind_cfg, &poa_cfg).await?;
            let cometbft_rpc_factory = create_cometbft_factory(&poa_cfg);
            let btc_server_factory = btc_server_client.unzip().0;
            let (abci_started_tx, abci_started_rx) = tokio::sync::oneshot::channel::<()>();

            // =========================================FROST TASK (WIP)=================================================
            let (frost_task, abci_client_builder, snapshot_manager, wallet_sync) =
                match AuthorityConsensusBuilder::try_new(
                    chain_spec_arc.clone(),
                    blockchain_provider.clone(),
                    activation_manager,
                    btc_server_factory,
                    bitcoin_checkpoints.clone(),
                    frost_setup_result.secret_key,
                    network_handle.clone(),
                    frost_handle,
                    task_executor.clone(),
                    frost_setup_result.frost_config,
                    bitcoind_cfg.btc_network,
                    frost_setup_result.genesis_authorities.clone(),
                    frost_setup_result.authorities_socket_addresses,
                    bitcoind_factory.clone(),
                    botanix_evm_config,
                    cometbft_rpc_factory,
                    RandomSourceProvider::new(),
                    driver_tx,
                    state_sync_cfg.clone(),
                    reth_provider_factory.clone(),
                    botanix_db_provider_factory,
                    poa_cfg.block_fee_recipient_address,
                    bitcoind_client,
                ) {
                    std::result::Result::Ok(consensus) => consensus.build::<BtcServerExtendedClient>().await,
                    std::result::Result::Err(e) => {
                        return Err(eyre::eyre!("AuthorityConsensusBuilderError : {:?}", e));
                    }
                };

                if let Some(mut snapshot_manager) = snapshot_manager {
                    tracing::info!("Snapshot manager is enabled.");
                    node.task_executor.spawn_critical(
                        "Snapshot Manager",
                        Box::pin(async move {
                            if let Err(e) = snapshot_manager.run().await {
                                tracing::error!(target: "reth::cli", "Snapshot Manager Error: {:?}", e);
                            }
                        }),
                    );
                }

                if let Some(wallet_sync) = wallet_sync {
                    node.task_executor.spawn_critical(
                        "Wallet Sync",
                        Box::pin(async move {
                            if let Err(e) = wallet_sync.sync_wallet_state().await {
                                tracing::error!(target: "reth::cli", "Wallet Sync Error: {:?}", e);
                            }
                        }),
                    );
                }

                if poa_cfg.federation_mode {
                    node.task_executor.spawn_critical(
                        "Frost Task",
                        Box::pin(async move {
                            frost_task.expect("frost task exists").start_task(abci_started_rx).await;
                        }),
                    );
                }

                // NOTE: the node will block here until DKG has completed
                let abci_client_builder = abci_client_builder.expect("abci client builder exists");
                let fut = || async {
                    abci_client_builder
                        .start_server(
                            &node.task_executor.clone(),
                            node.pool.clone(),
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

            // ==========================================================================================

            // add metrics if necessary
            if let Some(metrics_listener_address) = metrics_args {
                // start the metrics server
                let hooks = Hooks::builder().build();
                tracing::info!(target: "reth::cli", "Starting metrics endpoint at {}", metrics_listener_address.to_string());
                let config = MetricServerConfig::new(
                    metrics_listener_address,
                    VersionInfo {
                        version: CARGO_PKG_VERSION,
                        build_timestamp: VERGEN_BUILD_TIMESTAMP,
                        cargo_features: "VERGEN_CARGO_FEATURES",
                        git_sha: VERGEN_GIT_SHA,
                        target_triple: "VERGEN_CARGO_TARGET_TRIPLE",
                        build_profile: "BUILD_PROFILE_NAME",
                    },
                    ChainSpecInfo {
                        name: chain_spec.chain().id().to_string(),
                    },
                    task_executor.clone(),
                    hooks,
                );
                MetricServer::new(config).serve().await?;
            }

            // launch the network manager task
            node.task_executor.spawn_critical("network p2p", network_manager);
            node.task_executor.spawn_critical("txpool p2p task", tx_pool_p2p);
            //node.task_executor.spawn_critical("eth request handler p2p task", eth_request_handler_p2p);

            // launch the bitcoin checkpoints synchronizer task
            node.task_executor.spawn_critical(
                "async bitcoin checkpoint chain synchronization task",
                checkpoints_synchronizer.sync(bitcoin_zmq_block_hash_stream),
            );
            tracing::info!(target: "reth::cli", "Spawned async bitcoin task for block headers");

            // send the signal that abci driver can start
            abci_started_tx.send(()).expect("abci started tx");
            let (tx, rx) = tokio::sync::oneshot::channel();
            node.task_executor.spawn_critical(
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
            node.task_executor.spawn_critical("stages metrics listener task", sync_metrics_listener);

            // Send notification to beacon engine
            engine_handle_tx.send(node.beacon_engine_handle.clone()).unwrap();

            // Block and wait for node termination
            node_exit_future.await
        },
    )?;

    Ok(())
}

fn lookup_head(blockchain_provider: &BlockchainProvider<NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>>) -> Head {
        let head = blockchain_provider
        .get_stage_checkpoint(StageId::Finish)
        .expect("get stage point")
        .unwrap_or_default()
        .block_number;

    let header = blockchain_provider
        .header_by_number(head)
        .expect("missing header by number, database corrupt")
        .expect("the header for the latest block is missing, database is corrupt");

    let total_difficulty = blockchain_provider
        .header_td_by_number(head)
        .expect("missing header by number, database corrupt")
        .expect("the total difficulty for the latest block is missing, database is corrupt");

    let hash = blockchain_provider
        .block_hash(head)
        .expect("is some")
        .expect("the hash for the latest block is missing, database is corrupt");

    Head {
        number: head,
        hash,
        difficulty: header.difficulty,
        total_difficulty,
        timestamp: header.timestamp,
    }
}