
#![warn(unused_crate_dependencies)]

use std::{path::Path, sync::Arc};

use botanix_chainspec::BotanixChainSpec;
use reth::{api::NodeTypesWithDBAdapter, args::DatadirArgs};
use reth_chainspec::ChainSpecBuilder;
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
use reth_provider::providers::BlockchainProvider;
use reth_db_common::init::init_genesis;
use crate::node::BotanixNode;

pub fn create_blockchain_provider(
    chain_spec_arc: Arc<BotanixChainSpec>,
    datadir_args: &DatadirArgs,
    reth_database: Arc<DatabaseEnv>,
) -> eyre::Result<BlockchainProvider<NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>>> {
    let data_dir = datadir_args.datadir.unwrap_or_chain_default(chain_spec_arc.chain, datadir_args.clone());
    let reth_static_files_path = data_dir.static_files();
    let spec = Arc::new(ChainSpecBuilder::mainnet().build());
    let reth_provider_factory = ProviderFactory::<NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>>::new(
        reth_database,
        chain_spec_arc,
        StaticFileProvider::read_write(reth_static_files_path)?,
    );
    let genesis_hash = init_genesis(reth_provider_factory.clone())?;
    tracing::info!(target: "reth::cli", "Genesis hash: {}", genesis_hash);
    let provider = BlockchainProvider::new(reth_provider_factory)?;
    Ok(provider)
}
