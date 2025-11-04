use crate::{node::BotanixNode, BotanixPrimitives};
use botanix_chainspec::BotanixChainSpec;
use reth::{api::NodeTypesWithDBAdapter, args::DatadirArgs};
use reth_db_common::init::init_genesis;
use reth_ethereum::provider::{
    db::DatabaseEnv, providers::StaticFileProvider, ProviderFactory,
};
use reth_provider::providers::BlockchainProvider;
use std::sync::Arc;

/// Creates and initializes the reth blockchain provider, static file provider, and provider factory.
///
/// Returns a tuple (BlockchainProvider, StaticFileProvider, ProviderFactory) wrapped in `eyre::Result`.
///
/// - `chain_spec_arc`: Arc pointing to the chain specification.
/// - `datadir_args`: Datadir arguments used to determine storage locations.
/// - `reth_database`: Arc to the reth database environment.
pub fn create_blockchain_provider(
    chain_spec_arc: Arc<BotanixChainSpec>,
    datadir_args: &DatadirArgs,
    reth_database: Arc<DatabaseEnv>,
) -> eyre::Result<(
    BlockchainProvider<NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>>,
    StaticFileProvider<BotanixPrimitives>,
    ProviderFactory<NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>>,
)> {
    let data_dir = datadir_args
        .datadir
        .unwrap_or_chain_default(chain_spec_arc.chain, datadir_args.clone());
    let reth_static_files_path = data_dir.static_files();
    let static_files_provider =
        StaticFileProvider::read_write(reth_static_files_path)?;
    let reth_provider_factory = ProviderFactory::<
        NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>,
    >::new(
        reth_database,
        chain_spec_arc,
        static_files_provider.clone(),
    );
    let genesis_hash = init_genesis(&reth_provider_factory.clone())?;
    tracing::info!(target: "reth::cli", "Genesis hash: {}", genesis_hash);
    let provider = BlockchainProvider::new(reth_provider_factory.clone())?;
    Ok((provider, static_files_provider, reth_provider_factory))
}
