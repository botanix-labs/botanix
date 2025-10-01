//! Botanix Reth node entry point.
//!
//! This crate provides the main entry point for running a Botanix Reth node with Botanix support.

use std::path::PathBuf;
use botanix_cli_args::{poa_node::PoaNodeArgs, BotanixArgs};
use botanix_utils::panic_hook::set_panic_hook;
use clap::Parser;
use eyre::Context;
use reth::{args::NetworkArgs, cli::{Cli, Commands}};
use reth_botanix::{
    chainspec::parser::BotanixChainSpecParser,
    node::{consensus::BotanixConsensus, evm::config::BotanixEvmConfig, BotanixNode},
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

    cli.run_with_components::<BotanixNode>(
    |spec| (BotanixEvmConfig::new(spec.clone()), BotanixConsensus::new(spec)),
        async move |builder, args: BotanixArgs| {
            // Now you can use your custom Botanix args *and* the standard Reth NetworkArgs here:
            let _bitcoind_cfg = args.bitcoind.clone();
            let _frost_cfg    = args.frost.clone();
            let _poa_cfg      = args.poa.clone();

            tracing::info!(
                target: "botanix",
                "p2p addr={} port={} trusted_only={}",
                network_args.addr, network_args.port, network_args.trusted_only
            );

            // Bitcoind Config
            let _bitcoind_cfg = args.bitcoind.clone();

            // Frost Config
            let _frost_cfg = args.frost.clone();

            // POA Config
            let _poa_cfg = args.poa.clone();

            let _reth_cfg = load_reth_config(&args.poa, &network_args)?;
            let (node, engine_handle_tx) = BotanixNode::new();
            let reth::builder::NodeHandle { node, node_exit_future } =
                builder.node(node).launch().await?;

            engine_handle_tx.send(node.beacon_engine_handle.clone()).unwrap();
            node_exit_future.await
        },
    )?;

    Ok(())
}

fn load_reth_config(poa_args: &PoaNodeArgs, network_args: &NetworkArgs) -> eyre::Result<reth_config::Config> {
    match <std::option::Option<PathBuf> as Clone>::clone(&poa_args.network_config_path) {
        Some(config_path) => {
            let mut config = confy::load_path::<reth_config::Config>(&config_path)
                .wrap_err_with(|| format!("Could not load config file {:?}", config_path))?;

            tracing::info!(target: "reth::cli", path = ?config_path, "Network onfiguration loaded");

            // Update the config with the command line arguments
            config.peers.trusted_nodes_only = network_args.trusted_only;

            if !network_args.trusted_peers.is_empty() {
                tracing::info!(target: "reth::cli", "Adding trusted nodes");
                network_args.trusted_peers.iter().for_each(|peer| {
                    config.peers.trusted_nodes.push(peer.clone());
                });
            }
            Ok(config)
        }
        None => Ok(reth_config::Config::default()),
    }
}