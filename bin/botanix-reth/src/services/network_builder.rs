use std::sync::Arc;
use crate::{
    node::BotanixNode, services::frost::FrostConfigSetupResult
};
use botanix_cli_args::poa_node::PoaNodeArgs;
use reth::{args::{DatadirArgs, NetworkArgs}, providers::HeaderProvider};
use reth_chainspec::ChainSpec;
use reth_config::Config;
use reth_db::DatabaseEnv;
use reth_network::{NetworkConfigBuilder, NetworkManager};
use reth_node_builder::{components::PoolBuilder, NodeTypesWithDBAdapter};
use reth_provider::{providers::BlockchainProvider, BlockHashReader, StageCheckpointReader};
use reth_stages::StageId;
use alloy_eip2124::Head;
use reth_tasks::TaskExecutor;
use reth_transaction_pool::TransactionPool;

/// Look up the current chain head from the given blockchain provider.
///
/// Returns an `alloy_eip2124::Head` with the head number, hash, difficulty,
/// total difficulty and timestamp retrieved from the provider.
pub fn lookup_head(blockchain_provider: &BlockchainProvider<NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>>) -> Head {
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


pub async fn setup_network_builder(
        frost_setup_result: &FrostConfigSetupResult,
        node: &BotanixNode,
        reth_provider_factory: Arc<reth_provider::ProviderFactory<NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>>>,
        blockchain_provider: &BlockchainProvider<NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>>,
        reth_cfg: &Config,
        chain_spec_arc: &Arc<ChainSpec>,
        poa_cfg: &PoaNodeArgs,
        network_args: &NetworkArgs,
        datadir_args: &DatadirArgs,
        task_executor: TaskExecutor,
        pool: Pool<BotanixNode, Arc<DatabaseEnv>>,
) -> eyre::Result<()> {
    let secret_key = frost_setup_result.secret_key.clone();
    let data_dir = datadir_args.datadir.unwrap_or_chain_default(chain_spec_arc.chain, datadir_args.clone());
    let default_peers_path = data_dir.known_peers();
    let head = lookup_head(&blockchain_provider);

    let mut network_cfg_builder: NetworkConfigBuilder = network_args
        .network_config(&reth_cfg, chain_spec_arc.clone(), secret_key, default_peers_path)
        .with_task_executor(Box::new(task_executor))
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
            .transactions(pool, Default::default())
            .split_with_handle();

    // Ok((network_handle, network_manager, tx_pool_p2p, eth_request_handler_p2p, frost_p2p))
    Ok(())
}