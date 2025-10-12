
use std::{net::SocketAddr, sync::Arc};
use futures::TryFutureExt;
use botanix_chainspec::BotanixChainSpec;
use reth::{args::RpcServerArgs, tasks::TaskExecutor};
use reth_ethereum::{
    network::api::noop::NoopNetwork,
    node::api::NodeTypesWithDBAdapter,
    pool::noop::NoopTransactionPool,
    provider::{
        db::DatabaseEnv,
        providers::BlockchainProvider,
    },
    rpc::{
        builder::{RethRpcModule, RpcModuleBuilder, RpcServerConfig, TransportRpcModuleConfig},
        EthApiBuilder,
    },
};
use crate::{node::{evm::config::BotanixEvmConfig, BotanixNode}, services::rpc_impl::{MyRpcExt, MyRpcExtApiServer}};

pub async fn setup_rpc(
    provider: BlockchainProvider<NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>>,
    rpc_server_args: &RpcServerArgs,
    task_executor: &TaskExecutor,
    chain_spec: Arc<BotanixChainSpec>,
) -> eyre::Result<()> {
    let rpc_builder = RpcModuleBuilder::default()
        .with_provider(provider.clone())
        .with_noop_pool()
        .with_noop_network()
        .with_executor(Box::new(task_executor.clone()))
        .with_evm_config(BotanixEvmConfig::new(chain_spec.clone()));

    let eth_api = EthApiBuilder::new(
        provider.clone(),
        NoopTransactionPool::default(),
        NoopNetwork::default(),
        BotanixEvmConfig::new(chain_spec),
    )
    .build();

    // Pick which namespaces to expose.
    let module_config = TransportRpcModuleConfig::default().with_http([RethRpcModule::Eth]);

    let mut server = rpc_builder.build(module_config, eth_api);

    // Add a custom rpc namespace
    let custom_rpc = MyRpcExt { provider };
    server.merge_configured(custom_rpc.into_rpc())?;

    // Start the server & keep it alive
    let mut server_config = RpcServerConfig::default();

    // Configure HTTP if enabled
    if rpc_server_args.http {
        let http_socket_addr = SocketAddr::new(
            rpc_server_args.http_addr,
            rpc_server_args.http_port,
        );
        server_config = server_config.with_http_address(http_socket_addr);
    }

    if rpc_server_args.ws {
        let ws_socket_addr = SocketAddr::new(
            rpc_server_args.ws_addr,
            rpc_server_args.ws_port,
        );
        server_config = server_config.with_ws_address(ws_socket_addr);
    }

    server_config = server_config.with_ipc_endpoint(rpc_server_args.ipcpath.clone());

    let launch_rpc = server_config.start(&server).map_ok(|handle| {
        if let Some(path) = handle.ipc_endpoint() {
            tracing::info!(target: "reth::cli", %path, "RPC IPC server started");
        }
        if let Some(addr) = handle.http_local_addr() {
            tracing::info!(target: "reth::cli", url=%addr, "RPC HTTP server started");
        }
        if let Some(addr) = handle.ws_local_addr() {
            tracing::info!(target: "reth::cli", url=%addr, "RPC WS server started");
        }
        handle
    });

    launch_rpc.await?;

    Ok(())
}