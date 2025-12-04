use crate::{
    node::{
        evm::config::BotanixEvmConfig, primitives::BotanixPrimitives,
        BotanixNode,
    },
    services::{
        network_builder::{BotanixNetworkHandle, BotanixPool},
        rpc::botanixrpc_ext::{BotanixRpcExt, BotanixRpcExtApiServer},
    },
    BotanixBlock,
};
use botanix_chainspec::BotanixChainSpec;
use botanix_rpc_config::botanix_config::Botanix;
use futures::TryFutureExt;
use reth::{
    args::RpcServerArgs, rpc::builder::config::RethRpcServerConfig,
    tasks::TaskExecutor,
};
use reth_consensus::{Consensus, ConsensusError, FullConsensus};
use reth_ethereum::{
    node::api::NodeTypesWithDBAdapter,
    provider::{db::DatabaseEnv, providers::BlockchainProvider},
    rpc::{
        builder::{
            RethRpcModule, RpcModuleBuilder, RpcServerHandle,
            TransportRpcModuleConfig,
        },
        EthApiBuilder,
    },
};
use std::{net::SocketAddr, sync::Arc};

/// Sets up and runs the RPC server for the Botanix node, wiring providers,
/// network and transaction pool, configuring transports (HTTP/WS/IPC), and
/// starting the server; returns an error if server startup fails.
pub async fn setup_and_run_rpc<C>(
    provider: BlockchainProvider<
        NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>,
    >,
    rpc_server_args: &RpcServerArgs,
    task_executor: &TaskExecutor,
    chain_spec: Arc<BotanixChainSpec>,
    botanix_provider: Botanix,
    pool: BotanixPool,
    network: BotanixNetworkHandle,
    consensus: C,
) -> eyre::Result<RpcServerHandle>
where
    C: Consensus<BotanixBlock, Error = ConsensusError>
        + FullConsensus<BotanixPrimitives>
        + Clone
        + 'static,
{
    let rpc_builder = RpcModuleBuilder::default()
        .with_provider(provider.clone())
        .with_pool(pool.clone())
        .with_network(network.clone())
        .with_executor(Box::new(task_executor.clone()))
        .with_consensus(consensus)
        .with_evm_config(BotanixEvmConfig::new(chain_spec.clone()));

    let eth_api = EthApiBuilder::new(
        provider.clone(),
        pool,
        network,
        BotanixEvmConfig::new(chain_spec),
    )
    .build();

    // Pick which namespaces to expose.
    let module_config = TransportRpcModuleConfig::default()
        .with_http(RethRpcModule::all_variants())
        .with_ws(RethRpcModule::all_variants())
        .with_ipc(RethRpcModule::all_variants());

    let mut server = rpc_builder.build(module_config, eth_api);

    // Add a custom rpc namespace
    let custom_rpc = BotanixRpcExt { provider, botanix: botanix_provider };
    server.merge_configured(custom_rpc.into_rpc())?;

    // Start the server
    let server_config = rpc_server_args.rpc_server_config();

    let handle = server_config.start(&server).await?;

    if let Some(path) = handle.ipc_endpoint() {
        tracing::info!(target: "reth::cli", %path, "RPC IPC server started");
    }
    if let Some(addr) = handle.http_local_addr() {
        tracing::info!(target: "reth::cli", url=%addr, "RPC HTTP server started");
    }
    if let Some(addr) = handle.ws_local_addr() {
        tracing::info!(target: "reth::cli", url=%addr, "RPC WS server started");
    }

    Ok(handle)
}
