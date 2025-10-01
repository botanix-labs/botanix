
use std::{path::Path, sync::Arc};

use reth_ethereum::{
    chainspec::ChainSpecBuilder,
    consensus::EthBeaconConsensus,
    network::api::noop::NoopNetwork,
    node::{api::NodeTypesWithDBAdapter, EthEvmConfig, EthereumNode},
    pool::noop::NoopTransactionPool,
    provider::{
        db::{mdbx::DatabaseArguments, open_db_read_only, ClientVersion, DatabaseEnv},
        providers::{BlockchainProvider, StaticFileProvider},
        ProviderFactory,
    },
    rpc::{
        builder::{RethRpcModule, RpcModuleBuilder, RpcServerConfig, TransportRpcModuleConfig},
        EthApiBuilder,
    },
    tasks::TokioTaskExecutor,
};

use crate::node::BotanixNode;

// pub fn setup_rpc(provider: BlockchainProvider<NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>>) -> eyre::Result<RpcServerConfig> {
//     let rpc_builder = RpcModuleBuilder::default()
//         .with_provider(provider.clone())
//         // Rest is just noops that do nothing
//         .with_noop_pool()
//         .with_noop_network()
//         .with_executor(Box::new(TokioTaskExecutor::default()))
//         .with_evm_config(EthEvmConfig::new(spec.clone()))
//         .with_consensus(EthBeaconConsensus::new(spec.clone()));

//     let eth_api = EthApiBuilder::new(
//         provider.clone(),
//         NoopTransactionPool::default(),
//         NoopNetwork::default(),
//         EthEvmConfig::mainnet(),
//     )
//     .build();

//     // Pick which namespaces to expose.
//     let config = TransportRpcModuleConfig::default().with_http([RethRpcModule::Eth]);

//     let mut server = rpc_builder.build(config, eth_api);

//     // Add a custom rpc namespace
//     let custom_rpc = MyRpcExt { provider };
//     server.merge_configured(custom_rpc.into_rpc())?;

//     // Start the server & keep it alive
//     let server_args = RpcServerConfig::http(Default::default()).with_http_address("0.0.0.0:8545".parse()?);

//     Ok(server_args)
// }