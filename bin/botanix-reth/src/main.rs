//! Botanix Reth node entry point.
//!
//! This crate provides the main entry point for running a Botanix Reth node with Botanix support.

use std::sync::Arc;
use botanix_chainspec::{constants::{BOTANIX_MAINNET_CHAIN_ID, BOTANIX_TESTNET_CHAIN_ID}, parser::BotanixChainSpecParser};
use botanix_cli_args::{chain::{get_chain_from_federation_config, BotanixNetwork}, BotanixArgs};
use botanix_utils::panic_hook::set_panic_hook;
use clap::Parser;
use eyre::Ok;
use reth::{args::{NetworkArgs, RpcServerArgs}, cli::{Cli, Commands}};
use reth_botanix::{
    node::{consensus::BotanixConsensus, evm::config::BotanixEvmConfig, BotanixNode}, services::{activation_manager::setup_activation_manager, bitcoin_checkpoints::setup_bitcoin_checkpoints, bitcoind::setup_bitcoind_client, btc_server::create_btc_server_client, frost::setup_frost, migrator::init_and_migrate_db, provider::create_blockchain_provider, recover_utxos::recover_missing_utxos, reth::load_reth_config, rpc::setup_rpc},
};
use reth_cli_commands::NodeCommand;
use reth_node_core::version::version_metadata;

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
    let network_args: NetworkArgs = node_cmd.network.clone();
    let rpc_args: RpcServerArgs = node_cmd.rpc.clone();
    let datadir_args = node_cmd.datadir.clone();
    let chain = node_cmd.chain.clone();
    let db_args = node_cmd.db.clone();

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
            let reth_cfg = load_reth_config(&args.poa, &network_args)?;

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
            let bitcoind_client = setup_bitcoind_client(&bitcoind_cfg, &poa_cfg).await?;

            // Migrate the db if needed
            let (reth_database, botanix_database) = init_and_migrate_db(
                &datadir_args,
                Arc::clone(&chain_spec_arc),
                &db_args
            )?;

            // Create a blockchain provider
            let blockchain_provider = create_blockchain_provider(
                chain_spec_arc.clone(),
                &datadir_args,
                reth_database.clone()
            );

            // Create and connect to btc signining server if in federation mode
            let mut btc_server_client = create_btc_server_client(&poa_cfg, &bitcoind_cfg).await?;

            // Eventually we want to recover missing UTXOs on every start
            if let Some(mut btc_server_client) = btc_server_client.as_mut() {
                recover_missing_utxos(&poa_cfg, &mut btc_server_client).await?;
            }

            // Setup the activation manager
            let activation_manager = setup_activation_manager(&botanix_network);

            // Create frost manager
            let frost_config = setup_frost(
                &chain,
                &datadir_args,
                &poa_cfg,
                &network_args,
                &frost_cfg,
                &state_sync_cfg,
                &mut reth_cfg,
            )?;

            // Setup bitcoin checkpoints synchronizer
            let (checkpoints_synchronizer, bitcoin_zmq_block_hash_stream) = setup_bitcoin_checkpoints(
                bitcoind_client,
                &bitcoind_cfg,
                &chain,
            ).await?;

            // Setup the RPC server
            setup_rpc(
                blockchain_provider.clone(),
            ).await?;

            // build the node
            let (node, engine_handle_tx) = BotanixNode::new();

            // launch the node
            let reth::builder::NodeHandle { node, node_exit_future } =
                builder.node(node).launch().await?;

            // launch the bitcoin checkpoints synchronizer task
            node.task_executor.spawn_critical(
                "async bitcoin checkpoint chain synchronization task",
                checkpoints_synchronizer.sync(bitcoin_zmq_block_hash_stream),
            );
            tracing::info!(target: "reth::cli", "Spawned async bitcoin task for block headers");

            // Send notification to beacon engine
            engine_handle_tx.send(node.beacon_engine_handle.clone()).unwrap();

            // Block and wait for node termination
            node_exit_future.await
        },
    )?;

    Ok(())
}