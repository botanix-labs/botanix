use crate::{
    consensus::{
        comet_bft::abci::{ABCIClientBuilder, ABCIDriverMessage},
        frost_task::FrostTask,
        snapshot_manager::{SnapshotManager, SnapshotManagerStateLock},
        utils::{is_poa_epoch, seal_slow},
        wallet_state_sync::WalletStateSyncEngine,
        Storage,
    },
    node::{
        consensus::BotanixConsensus, evm::config::BotanixEvmConfig,
        network::BotanixNetworkPrimitives, BotanixNode,
    },
};
use alloy_primitives::Address;
use botanix_activation_manager::{ActivationManager, VoteWatcher};
use botanix_authority_edh::header_ext::HeaderExt;
use botanix_authority_metrics::AuthorityMetrics;
use botanix_authority_rsp::RandomSource;
use botanix_bitcoin_checkpoint::BitcoinCheckpointsChain;
use botanix_btc_server_client::{
    BtcServerExtendedApi, BtcServerExtendedClient, Empty, GetPublicKeyRequest,
    GrpcClientFactory, SubscribeToDynafedNotificationsStream,
};
use botanix_btc_wallet::fallback::FallbackBitcoindClient;
use botanix_chainspec::BotanixChainSpec;
use botanix_cli_args::state_sync::StateSyncArgs;
use botanix_comet_bft_rpc::{
    Client, CometBftRpcFactory, HttpCometBFTRpcClientFactory,
};
use botanix_data_parser::{DataParser, SerializationType};
use botanix_storage::{
    RuntimeTransitionsReadWrite, SnapshotReader, SnapshotWriter,
    StagedHeaderReader, StagedHeaderWriter, WalletStateSyncReader,
    WalletStateSyncWriter,
};
use botanix_types::MultisigId;
use futures::{pin_mut, StreamExt};
use reth_db::DatabaseEnv;
use reth_network::{
    frost::manager::{FrostConfig, ToFrostManager},
    NetworkHandle,
};
use reth_node_builder::NodeTypesWithDBAdapter;
use reth_primitives::NodePrimitives;
use reth_provider::{
    BlockReaderIdExt, CanonChainTracker, CanonStateSubscriptions,
    ProviderFactory, StateProviderFactory,
};
use reth_storage_api::NodePrimitivesProvider;
use reth_tasks::TaskExecutor;
use std::{
    net::SocketAddr,
    str::FromStr,
    sync::{Arc, RwLock},
    time::Duration,
};
use tracing::{info, warn};

/// Builder type for configuring the setup
#[allow(dead_code)]
pub struct AuthorityConsensusBuilder<RDB, BDB, ToFrostMan, Source> {
    consensus: BotanixConsensus<BotanixChainSpec>,
    storage: Storage<RDB, BDB>,
    activation_manager: ActivationManager<VoteWatcher, Address>,
    btc_server_factory: Option<GrpcClientFactory>,
    bitcoin_checkpoints: Arc<BitcoinCheckpointsChain>,
    network_handle: NetworkHandle<BotanixNetworkPrimitives>,
    frost_handle: Option<ToFrostMan>,
    task_executor: TaskExecutor,
    frost_config: Option<FrostConfig>,
    cometbft_rpc_factory: HttpCometBFTRpcClientFactory,
    random_source_provider: Source,
    metrics: Arc<AuthorityMetrics>,
    abci_driver_tx: tokio::sync::mpsc::Sender<ABCIDriverMessage>,
    reth_provider_factory:
        ProviderFactory<NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>>,
    state_sync: StateSyncArgs,
    block_fee_recipient_address: Option<alloy_primitives::Address>,
    bitcoind_client: Arc<FallbackBitcoindClient>,
}

/// Errors that can occur when building an authority consensus.
#[derive(Debug)]
pub enum AuthorityConsensusBuilderError {
    InvalidStorage,
    FailedToRecoverAuthorityList,
    FailedToFindSignerIndex,
    FailedToRetrieveEopchHeader,
}

// ===== impl AuthorityConsensusBuilder =====
impl<RDB, BDB, ToFrostMan, Source>
    AuthorityConsensusBuilder<RDB, BDB, ToFrostMan, Source>
where
    ToFrostMan: ToFrostManager + Clone + 'static + Send + Sync,
    RDB: BlockReaderIdExt<Header = alloy_consensus::Header>
        + StateProviderFactory
        + Clone
        + CanonChainTracker
        + CanonStateSubscriptions
        + reth_provider::ChainSpecProvider<
            ChainSpec: reth_chainspec::EthereumHardforks,
        > + 'static,
    BDB: SnapshotReader
        + SnapshotWriter
        + WalletStateSyncWriter
        + WalletStateSyncReader
        + StagedHeaderReader
        + StagedHeaderWriter
        + RuntimeTransitionsReadWrite
        + Clone
        + 'static,
    Source: RandomSource,
{
    /// Creates a new builder instance to configure all parts.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        chain_spec: Arc<BotanixChainSpec>,
        reth_provider: RDB,
        activation_manager: ActivationManager<VoteWatcher, Address>,
        btc_server_factory: Option<GrpcClientFactory>,
        bitcoin_checkpoints: Arc<BitcoinCheckpointsChain>,
        sk: secp256k1::SecretKey,
        network_handle: NetworkHandle<BotanixNetworkPrimitives>,
        frost_handle: Option<ToFrostMan>,
        task_executor: TaskExecutor,
        frost_config: Option<FrostConfig>,
        btc_network: bitcoin::Network,
        genesis_authorities: Vec<secp256k1::PublicKey>,
        authority_socket_addresses: Vec<SocketAddr>,
        evm_config: BotanixEvmConfig,
        cometbft_rpc_factory: HttpCometBFTRpcClientFactory,
        random_source_provider: Source,
        abci_driver_tx: tokio::sync::mpsc::Sender<ABCIDriverMessage>,
        state_sync: StateSyncArgs,
        reth_provider_factory: ProviderFactory<
            NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>,
        >,
        botanix_provider_factory: BDB,
        block_fee_recipient_address: Option<alloy_primitives::Address>,
        bitcoind_client: Arc<FallbackBitcoindClient>,
    ) -> Result<Self, AuthorityConsensusBuilderError> {
        // only a federation node has a btc_server
        let is_fed_node = btc_server_factory.is_some();

        // Check the local database if a runtime upgrade has occurred which the
        // ActivationManager does not know about.
        if let Some(runtime_version) = botanix_provider_factory
            .get_last_runtime_version()
            .expect("local db must be available")
        {
            let was_forced =
                activation_manager.force_upgrade_checked(runtime_version);
            if was_forced {
                warn!("Detected completed network upgrade to version '{runtime_version}' that was unknown to initiated ActivationManager");
            }
        }

        let mut latest_header = reth_provider
            .latest_header()
            .ok()
            .flatten()
            .unwrap_or_else(|| chain_spec.inner().sealed_genesis_header());
        let mut headers = vec![latest_header.clone()];

        while !is_poa_epoch(
            latest_header.header().number,
            chain_spec.epoch_length,
        ) {
            let parent_hash = latest_header.parent_hash;

            if let Some(new_header) =
                reth_provider.header(&parent_hash).ok().flatten()
            {
                let old_latest_header = std::mem::replace(
                    &mut latest_header,
                    seal_slow(&new_header),
                );
                headers.push(old_latest_header);
            } else {
                return Err(
                    AuthorityConsensusBuilderError::FailedToRetrieveEopchHeader,
                );
            }
        }

        let agg_pk = {
            if latest_header.number > 0 {
                Some(
                    latest_header
                        .header()
                        .get_aggregate_public_key()
                        .expect("latest header is greater than genesis"),
                )
            } else {
                None
            }
        };
        info!("Aggregate public key: {:?}", agg_pk);

        // Authority length represents a non federation node since it would be
        // out of bounds. This prevents the node from signing blocks
        // although there are other checks to stop this as well.
        let mut signer_index = Some(genesis_authorities.len() + 1);
        // only a federation node has a btc_server
        if is_fed_node {
            signer_index = genesis_authorities
                .iter()
                .position(|a| *a == sk.public_key(secp256k1::SECP256K1));

            if signer_index.is_none() {
                return Err(
                    AuthorityConsensusBuilderError::FailedToFindSignerIndex,
                );
            }
        }
        let pk = sk.public_key(secp256k1::SECP256K1);

        // Try to instantiate storage
        let storage = Storage::new(
            genesis_authorities,
            signer_index.expect("valid index"),
            pk,
            btc_network,
            // Aggregate pk to be filled out by the dkg state machine if we are
            // still on genesis block
            agg_pk,
            authority_socket_addresses,
            evm_config,
            chain_spec.clone(),
            bitcoind_client.clone(),
            reth_provider,
            botanix_provider_factory,
        );

        Ok(Self {
            storage,
            activation_manager,
            consensus: BotanixConsensus::new(chain_spec),
            btc_server_factory,
            bitcoin_checkpoints,
            network_handle,
            frost_handle,
            task_executor,
            frost_config,
            cometbft_rpc_factory,
            random_source_provider,
            metrics: Arc::new(AuthorityMetrics::default()),
            abci_driver_tx,
            reth_provider_factory,
            state_sync,
            block_fee_recipient_address,
            bitcoind_client,
        })
    }

    /// Builds and returns the necessary components for the authority consensus,
    /// including the consensus itself, the client used to interact with the
    /// consensus, and the block production task.
    pub async fn build<BtcServerClient>(
        self,
    ) -> (
        Option<FrostTask<RDB, BDB, ToFrostMan, Source, BtcServerClient>>,
        Option<ABCIClientBuilder<RDB, BDB>>,
        Option<SnapshotManager<RDB, BDB>>,
        Option<WalletStateSyncEngine<RDB, BDB, ToFrostMan, BtcServerClient>>,
        BotanixConsensus<BotanixChainSpec>,
    )
    where
        BtcServerClient: BtcServerExtendedApi + Clone + Send + Sync + 'static,
        BtcServerExtendedClient: Into<BtcServerClient>,
        <<RDB as NodePrimitivesProvider>::Primitives as NodePrimitives>::BlockHeader: HeaderExt,
    {
        let Self {
            btc_server_factory,
            consensus,
            storage,
            activation_manager,
            bitcoin_checkpoints,
            network_handle,
            frost_handle,
            task_executor,
            frost_config,
            cometbft_rpc_factory,
            random_source_provider,
            metrics,
            abci_driver_tx,
            reth_provider_factory,
            state_sync,
            block_fee_recipient_address,
            bitcoind_client,
        } = self;

        let is_fed_node = btc_server_factory.is_some();
        let chain_spec = storage.chain_spec.clone();
        let parser = DataParser::default()
            .with_serialization_type(SerializationType::Postcard);

        let mut btc_server_client: Option<BtcServerClient> = async {
            if is_fed_node {
                Some(
                    btc_server_factory
                        .expect("btc_server_factory is available")
                        .build_and_connect()
                        .await
                        .expect("Failed to build and connect to btc server")
                        .into(),
                )
            } else {
                None
            }
        }
        .await;

        let wallet_sync = {
            if let Some(btc_server) = &btc_server_client {
                let wallet_state_sync_engine = WalletStateSyncEngine::new(
                    storage.clone(),
                    btc_server.clone(),
                    frost_handle.clone().expect("Requires frost handle"),
                    task_executor.clone(),
                    frost_config.clone().expect("frost config exists"),
                );
                Some(wallet_state_sync_engine)
            } else {
                None
            }
        };

        // create frost and block production tasks if btc_server is available:
        // only federation nodes will have btc_server
        let (dynafed_frost_notifications_tx, _) =
            tokio::sync::broadcast::channel::<
                SubscribeToDynafedNotificationsStream,
            >(100);
        let mut frost_task = None;
        if is_fed_node {
            // frost task
            let task = FrostTask::new(
                chain_spec.clone(),
                btc_server_client.clone().expect("btc_server is available"),
                network_handle.clone(),
                frost_handle.clone().expect("Requires frost handle"),
                frost_config.clone().expect("frost config exists"),
                storage.clone(),
                parser.clone(),
                random_source_provider,
                Arc::clone(&metrics),
                cometbft_rpc_factory.clone(),
                dynafed_frost_notifications_tx.clone(),
            );

            frost_task = Some(task);
        }

        let snapshot_manager_state_lock =
            Arc::new(RwLock::new(SnapshotManagerStateLock::default()));

        // all nodes will have an abci client builder
        let abci_client_builder = Some(ABCIClientBuilder::new(
            storage.clone(),
            activation_manager,
            bitcoin_checkpoints,
            consensus.clone(),
            cometbft_rpc_factory.clone(),
            is_fed_node,
            Arc::clone(&metrics),
            task_executor.clone(),
            parser.clone(),
            abci_driver_tx,
            reth_provider_factory.clone(),
            Arc::clone(&snapshot_manager_state_lock),
            state_sync.snapshot_message_format,
            block_fee_recipient_address,
        ));

        let snapshot_manager = if state_sync.enable_state_sync {
            Some(SnapshotManager::new(
                storage.clone(),
                parser.clone(),
                state_sync.num_snapshots_to_keep,
                state_sync.snapshot_message_format,
                state_sync.enable_state_sync,
                state_sync.enable_historical_sync,
                Arc::clone(&snapshot_manager_state_lock),
                cometbft_rpc_factory.clone(),
            ))
        } else {
            None
        };

        let mut btc_server_client_clone = btc_server_client.clone();
        task_executor.spawn_critical(
            "subscribe_to_dkg_notifications task",
            Box::pin(async move {

                let Some(btc) = btc_server_client_clone.as_mut() else {
                    return;
                };

                let dynafed_notifications_stream = match btc.subscribe_to_dynafed_notifications(Empty {}).await {
                    Ok(res) => {
                        info!(target: "reth::authority", "Btc server is healthy");
                        res
                    }
                    Err(e) => {
                        tracing::error!(target: "reth::authority", "Btc server is unhealthy: {}", e);
                        return;
                    }
                };

                pin_mut!(dynafed_notifications_stream);
                while let Some(msg) = dynafed_notifications_stream.next().await {
                    match msg {
                        Ok(msg) => {
                            info!(target: "reth::authority", "Received Dynafed notification from btc server");
                            match dynafed_frost_notifications_tx.send(msg) {
                                Ok(_) => {
                                    info!(target: "reth::authority", "Sent Dynafed notification to frost task");
                                }
                                Err(e) => {
                                    tracing::error!(target: "reth::authority", "Error sending Dynafed notification to frost task: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!(target: "reth::authority", "Error receiving Dynafed notification from btc server: {}", e);
                        }
                    }
                }
            })
        );

        // run a background health monitoring task for the btc server, comet and
        // bitcoind
        if is_fed_node {
            let mut btc_server_client = btc_server_client.clone();
            let cbft_rpc_provider =
                cometbft_rpc_factory.build_and_connect().unwrap();
            let metrics = Arc::clone(&metrics);
            task_executor.spawn_critical(
                "healthcheck monitoring task",
                Box::pin(async move {
                    loop {
                        // Health check for btc server
                        if let Some(btc) = btc_server_client.as_mut() {
                            match btc.health_check(Empty {}).await {
                                Ok(_) => {
                                    info!(target: "reth::authority", "Btc server is healthy");
                                    metrics.btc_server_connection_status.set(1);
                                }
                                Err(e) => {
                                    tracing::error!(target: "reth::authority", "Btc server is unhealthy: {}", e);
                                    metrics.btc_server_connection_status.set(0);
                                }
                            }
                        }
                        // Health check for bitcoind
                        match bitcoind_client.is_synced().await {
                            Ok(status) => {
                                tracing::info!(target: "reth::authority", "Bitcoind server is healthy");
                                if status { metrics.bitcoind_connection_status.set(1) } else { metrics.bitcoind_connection_status.set(0) };
                            }
                            Err(e) => {
                                tracing::error!(target: "reth::authority", "Bitcoind server is unhealthy: {}", e);
                                metrics.bitcoind_connection_status.set(0);
                            }
                        }

                        // Health check for cbft
                        match cbft_rpc_provider.health().await {
                            Ok(_) => {
                                tracing::info!(target: "reth::authority", "CometBFT server is healthy");
                                metrics.cometbft_connection_status.set(1);
                            }
                            Err(e) => {
                                tracing::error!(target: "reth::authority", "CometBFT server is unhealthy: {}", e);
                                metrics.cometbft_connection_status.set(0);
                            }
                        }
                        tokio::time::sleep(Duration::from_secs(60)).await;
                    }
                })
            );
        }

        // load all multisig ids and aggregated public keys into the storage
        if let Some(btc_server) = btc_server_client.as_mut() {
            let multisig_ids = match btc_server.list_multisigs(Empty {}).await {
                Ok(multisig_ids) => {
                    info!(target: "reth::authority", "Found {} multisig ids", multisig_ids.ids.len());
                    multisig_ids
                }
                Err(e) => {
                    tracing::error!(target: "reth::authority", "Error getting multisig ids: {}", e);
                    panic!("Error getting multisig ids: {}", e);
                }
            }
            .ids;

            let mut aggregated_pub_keys = vec![];
            for multisig_id in multisig_ids {
                match btc_server
                    .get_public_key(GetPublicKeyRequest { multisig_id })
                    .await
                {
                    Ok(resp) => {
                        if let Ok(pk) =
                            secp256k1::PublicKey::from_str(&resp.publickey)
                        {
                            let multisig_id: MultisigId = multisig_id.into();
                            aggregated_pub_keys.push((multisig_id, pk));
                        } else {
                            tracing::error!(target: "reth::authority", "Error parsing public key for multisig id: {}", multisig_id);
                            panic!(
                                "Error parsing public key for multisig id: {}",
                                multisig_id
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(target: "reth::authority", "Error retrieving public key for multisig id: {}, e = {}", multisig_id, e);
                        panic!("Error retrieving public key for multisig id: {}, e = {}", multisig_id, e);
                    }
                }
            }

            if !aggregated_pub_keys.is_empty() {
                let mut storage = storage.write().await;
                match storage.aggregate_public_key.as_mut() {
                    Some(storage_pub_keys) => {
                        storage_pub_keys.extend(aggregated_pub_keys)
                    }
                    None => {
                        storage.aggregate_public_key =
                            Some(aggregated_pub_keys.into_iter().collect())
                    }
                }
                drop(storage);
            }
        }

        (
            frost_task,
            abci_client_builder,
            snapshot_manager,
            wallet_sync,
            consensus,
        )
    }
}
