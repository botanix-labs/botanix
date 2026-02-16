use crate::{
    consensus::{
        frost_task::FrostTask,
        wallet_state_sync::{WalletStateSync, WalletStateSyncEngine},
        Storage,
    },
    node::network::BotanixNetworkPrimitives,
    services::frost::MultisigConfig,
};
use botanix_authority_edh::header_ext::HeaderExt;
use botanix_authority_metrics::AuthorityMetrics;
use botanix_authority_rsp::RandomSource;
use botanix_btc_server_client::{
    BtcServerExtendedApi, BtcServerExtendedClient, Empty, GetPublicKeyRequest,
    GrpcClientFactory, SubscribeToDynafedNotificationsStream,
};
use botanix_btc_wallet::fallback::FallbackBitcoindClient;
use botanix_cli_args::state_sync::WALLET_STATE_SYNC_CHUNK_SIZE;
use botanix_comet_bft_rpc::{
    Client, CometBftRpcFactory, HttpCometBFTRpcClientFactory,
};
use botanix_data_parser::{DataParser, SerializationType};
use botanix_storage::{
    StagedHeaderReader, StagedHeaderWriter, WalletStateSyncReader,
    WalletStateSyncWriter,
};
use botanix_types::{MultisigId, LEGACY_MULTISIG_ID};
use futures::{pin_mut, StreamExt};
use reth_network::{frost::manager::ToFrostManager, NetworkHandle};
use reth_primitives::NodePrimitives;
use reth_provider::{
    BlockReaderIdExt, CanonChainTracker, CanonStateSubscriptions,
    StateProviderFactory,
};
use reth_storage_api::NodePrimitivesProvider;
use reth_tasks::TaskExecutor;
use std::{str::FromStr, sync::Arc, time::Duration};
use tracing::info;

/// Builder type for configuring the setup
#[allow(dead_code)]
pub struct OperatorBuilder<RDB, BDB, ToFrostMan, Source> {
    storage: Storage<RDB, BDB>,
    btc_server_factory: GrpcClientFactory,
    network_handle: NetworkHandle<BotanixNetworkPrimitives>,
    frost_handle: ToFrostMan,
    task_executor: TaskExecutor,
    multisig_configs: Vec<MultisigConfig>,
    cometbft_rpc_factory: HttpCometBFTRpcClientFactory,
    random_source_provider: Source,
    metrics: Arc<AuthorityMetrics>,
    bitcoind_client: Arc<FallbackBitcoindClient>,
}

/// Errors that can occur when building an authority consensus.
#[derive(Debug)]
pub enum OperatorBuilderError {
    InvalidStorage,
    FailedToRecoverAuthorityList,
    FailedToFindSignerIndex,
    FailedToRetrieveEopchHeader,
}

impl<RDB, BDB, ToFrostMan, Source> OperatorBuilder<RDB, BDB, ToFrostMan, Source>
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
    BDB: StagedHeaderReader
        + StagedHeaderWriter
        + WalletStateSyncWriter
        + WalletStateSyncReader
        + Clone
        + 'static,
    Source: RandomSource + Clone,
{
    /// Creates a new builder instance to configure all parts.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        storage: Storage<RDB, BDB>,
        btc_server_factory: GrpcClientFactory,
        network_handle: NetworkHandle<BotanixNetworkPrimitives>,
        frost_handle: ToFrostMan,
        task_executor: TaskExecutor,
        multisig_configs: Vec<MultisigConfig>,
        cometbft_rpc_factory: HttpCometBFTRpcClientFactory,
        random_source_provider: Source,
        bitcoind_client: Arc<FallbackBitcoindClient>,
    ) -> Self {
        Self {
            storage,
            btc_server_factory,
            network_handle,
            frost_handle,
            task_executor,
            multisig_configs,
            cometbft_rpc_factory,
            random_source_provider,
            metrics: Arc::new(AuthorityMetrics::default()),
            bitcoind_client,
        }
    }

    /// Builds and returns the necessary components for the authority consensus,
    /// including the consensus itself, the client used to interact with the
    /// consensus, and the block production task.
    pub async fn build<BtcServerClient>(
        self,
    ) ->
        FrostTask<RDB, BDB, ToFrostMan, Source, BtcServerClient>
    where
        BtcServerClient: BtcServerExtendedApi + Clone + Send + Sync + 'static,
        BtcServerExtendedClient: Into<BtcServerClient>,
        <<RDB as NodePrimitivesProvider>::Primitives as NodePrimitives>::BlockHeader: HeaderExt,
    {
        let Self {
            btc_server_factory,
            storage,
            network_handle,
            frost_handle,
            task_executor,
            multisig_configs,
            cometbft_rpc_factory,
            random_source_provider,
            metrics,
            bitcoind_client,
        } = self;

        let parser = DataParser::default()
            .with_serialization_type(SerializationType::Postcard);

        let mut btc_server = btc_server_factory
            .build_and_connect()
            .await
            .expect("Failed to build and connect to btc server")
            .into();

        // TODO:
        let legacy = multisig_configs
            .iter()
            .find(|m| m.multisig_id == LEGACY_MULTISIG_ID)
            .unwrap();

        let wallet_sync = WalletStateSyncEngine::new(
            storage.clone(),
            btc_server.clone(),
            frost_handle.clone(),
            task_executor.clone(),
            legacy.min_signers as u64,
        );

        // create frost and block production tasks if btc_server is available:
        // only federation nodes will have btc_server
        let (dynafed_frost_notifications_tx, _) =
            tokio::sync::broadcast::channel::<
                SubscribeToDynafedNotificationsStream,
            >(100);

        // frost task
        let frost_task: FrostTask<_, _, _, _, _> = FrostTask::new(
            btc_server.clone(),
            network_handle.clone(),
            frost_handle.clone(),
            multisig_configs.clone(),
            storage.clone(),
            parser.clone(),
            random_source_provider,
            Arc::clone(&metrics),
            cometbft_rpc_factory.clone(),
            dynafed_frost_notifications_tx.clone(),
        );

        let btc_server_clone = btc_server.clone();
        task_executor.spawn_critical(
            "subscribe_to_dkg_notifications task",
            Box::pin(async move {
				let mut btc_server = btc_server_clone;

                let dynafed_notifications_stream = match btc_server.subscribe_to_dynafed_notifications(Empty {}).await {
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
        let btc_server_clone = btc_server.clone();

        let cbft_rpc_provider =
            cometbft_rpc_factory.build_and_connect().unwrap();

        let metrics = Arc::clone(&metrics);
        task_executor.spawn_critical(
			"healthcheck monitoring task",
			Box::pin(async move {
				let mut btc_server = btc_server_clone;

				loop {
					// Health check for btc server
					match btc_server.health_check(Empty {}).await {
						Ok(_) => {
							info!(target: "reth::authority", "Btc server is healthy");
							metrics.btc_server_connection_status.set(1);
						}
						Err(e) => {
							tracing::error!(target: "reth::authority", "Btc server is unhealthy: {}", e);
							metrics.btc_server_connection_status.set(0);
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

        task_executor.spawn_critical(
            "WalletSync",
            Box::pin(async move {
                if let Err(e) = wallet_sync.sync_wallet_state().await {
                    tracing::error!(target: "reth::cli", "Wallet Sync Error: {:?}", e);
                }
            }),
        );

        // TODO: Revisit this; the logic below assumes that only federation
        // members must know the multisigs, which is incorrect. This logic must
        // be applied to both regular nodes and federation members in
        // [`super::AuthorityConsensusBuilder`].
        //
        // load all multisig ids and aggregated public keys into the storage
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

        frost_task
    }
}
