use crate::{
    node::{network::BotanixNetworkPrimitives, BotanixNode},
    services::frost::FrostConfigSetupResult,
};
use alloy_eip2124::Head;
use botanix_cli_args::poa_node::PoaNodeArgs;
use reth::{
    args::{DatadirArgs, NetworkArgs},
    providers::HeaderProvider,
};
use reth_chainspec::ChainSpec;
use reth_config::Config;
use reth_db::DatabaseEnv;
use reth_network::{
    eth_requests::EthRequestHandler,
    frost::{
        manager::{FrostConfig, FrostManager},
        protocol::FrostProtoHandler,
    },
    protocol::IntoRlpxSubProtocol,
    transactions::TransactionsManager,
    NetworkConfigBuilder, NetworkHandle, NetworkManager,
};
use reth_network_peers::pk2id;
use reth_node_builder::NodeTypesWithDBAdapter;
use reth_provider::{
    providers::BlockchainProvider, BlockHashReader, ProviderFactory,
    StageCheckpointReader,
};
use reth_stages::StageId;
use reth_tasks::TaskExecutor;
use reth_transaction_pool::{
    blobstore::{DiskFileBlobStore, InMemoryBlobStore},
    CoinbaseTipOrdering, EthPooledTransaction, EthTransactionValidator,
    TransactionValidationTaskExecutor,
};
use secp256k1::SECP256K1;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;

pub type BotanixPool = reth::transaction_pool::Pool<
    TransactionValidationTaskExecutor<
        EthTransactionValidator<
            BlockchainProvider<
                NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>,
            >,
            EthPooledTransaction,
        >,
    >,
    CoinbaseTipOrdering<EthPooledTransaction>,
    InMemoryBlobStore,
>;
pub type BotanixNetworkHandle = NetworkHandle<BotanixNetworkPrimitives>;
pub type BotanixNetworkManager = NetworkManager<BotanixNetworkPrimitives>;
pub type BotanixTxPoolP2P = TransactionsManager<
    reth_transaction_pool::Pool<
        TransactionValidationTaskExecutor<
            EthTransactionValidator<
                BlockchainProvider<
                    NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>,
                >,
                EthPooledTransaction,
            >,
        >,
        CoinbaseTipOrdering<EthPooledTransaction>,
        InMemoryBlobStore,
    >,
    BotanixNetworkPrimitives,
>;
pub type BotanixEthRequestHandlerP2P = EthRequestHandler<
    ProviderFactory<NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>>,
    BotanixNetworkPrimitives,
>;
pub type FrostP2P = Option<FrostManager<BotanixNetworkPrimitives>>;

/// Look up the current chain head from the given blockchain provider.
///
/// Returns an `alloy_eip2124::Head` with the head number, hash, difficulty,
/// total difficulty and timestamp retrieved from the provider.
pub fn lookup_head(
    blockchain_provider: &BlockchainProvider<
        NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>,
    >,
) -> eyre::Result<Head> {
    let head = blockchain_provider
        .get_stage_checkpoint(StageId::Finish)?
        .unwrap_or_default()
        .block_number;

    let header = blockchain_provider
        .header_by_number(head)?
        .ok_or_else(|| eyre::eyre!("missing header for block {}", head))?;

    let total_difficulty =
        blockchain_provider.header_td_by_number(head)?.ok_or_else(|| {
            eyre::eyre!("missing total difficulty for block {}", head)
        })?;

    let hash = blockchain_provider
        .block_hash(head)?
        .ok_or_else(|| eyre::eyre!("missing hash for block {}", head))?;

    Ok(Head {
        number: head,
        hash,
        difficulty: header.difficulty,
        total_difficulty,
        timestamp: header.timestamp,
    })
}

/// Sets up the P2P network for the node and returns the network handle and manager.
pub async fn setup_network_builder(
    frost_setup_result: &FrostConfigSetupResult,
    reth_provider_factory: &reth_provider::ProviderFactory<
        NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>, // TODO: do we need a BotanixNode?
    >,
    blockchain_provider: &BlockchainProvider<
        NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>,
    >,
    reth_cfg: &Config,
    chain_spec_arc: &Arc<ChainSpec>,
    poa_cfg: &PoaNodeArgs,
    network_args: &NetworkArgs,
    datadir_args: &DatadirArgs,
    task_executor: TaskExecutor,
    pool: BotanixPool,
) -> eyre::Result<(
    BotanixNetworkHandle,
    BotanixNetworkManager,
    BotanixTxPoolP2P,
    BotanixEthRequestHandlerP2P,
    FrostP2P,
)> {
    let secret_key = frost_setup_result.secret_key.clone();
    let data_dir = datadir_args
        .datadir
        .unwrap_or_chain_default(chain_spec_arc.chain, datadir_args.clone());
    let default_peers_path = data_dir.known_peers();
    let head = lookup_head(&blockchain_provider)?;

    let mut network_cfg_builder: NetworkConfigBuilder<
        BotanixNetworkPrimitives,
    > = network_args
        .network_config::<BotanixNetworkPrimitives>(
            &reth_cfg,
            chain_spec_arc.clone(),
            secret_key,
            default_peers_path,
        )
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

    // Frost sub protocol is only supported by federation nodes
    if poa_cfg.federation_mode {
        let (protocol_events_tx, protocol_events_rx) =
            tokio::sync::mpsc::channel(10_000);
        let my_peer_id = pk2id(&secret_key.public_key(SECP256K1));
        let protocol_handler =
            FrostProtoHandler { my_peer_id, protocol_events_tx };

        network_cfg_builder = network_cfg_builder
            .frost_protocol_events_rx(ReceiverStream::new(protocol_events_rx))
            .add_rlpx_sub_protocol(protocol_handler.into_rlpx_sub_protocol());
    }

    // Optionally disable discovery if needed
    if network_args.trusted_only {
        network_cfg_builder = network_cfg_builder.disable_discovery();
    }

    // Set network mode to Authority if this is a validator/authority node
    if poa_cfg.federation_mode {
        network_cfg_builder = network_cfg_builder
            .network_mode(reth_network::config::NetworkMode::Authority);
    }
    let network_config =
        network_cfg_builder.build(reth_provider_factory.clone());

    let frost_config = if poa_cfg.federation_mode {
        // Prepare `FrostConfig` for the `NetworkManager`. Do NOTE that the
        // manager only requires the authority list, therefore we use garbage
        // values for the other fields.
        //
        // TODO (lamafab): Consider updating this upstream in the `reth` repo
        // such that there is no confusion about this behavior.
        let authorities = frost_setup_result.authorities();
        if authorities.is_empty() {
            return Err(eyre::eyre!("authority list is empty"));
        }

        Some(FrostConfig {
            authority_pk: authorities[0], // garbage
            authority_index: usize::MAX,  // garbage
            authorities,
            min_signers: u16::MIN, // garbage
            max_signers: u16::MIN, // garbage
            wallet_state_sync_chunk_size: u64::MIN, // garbage
        })
    } else {
        None
    };

    // Create the network manager and get the handle
    let (
        network_handle,
        network_manager,
        tx_pool_p2p,
        eth_request_handler_p2p,
        frost_p2p,
    ) = NetworkManager::builder(network_config)
        .await?
        .frost(frost_config)
        .request_handler(reth_provider_factory.clone())
        .transactions(pool, Default::default())
        .split_with_handle();

    Ok((
        network_handle,
        network_manager,
        tx_pool_p2p,
        eth_request_handler_p2p,
        frost_p2p,
    ))
}
