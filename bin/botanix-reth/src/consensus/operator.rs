use crate::{
    consensus::{
        frost_task::FrostTask,
        multisig_manager::MultisigSubmitter,
        wallet_state_sync::{WalletStateSync, WalletStateSyncEngine},
        Storage,
    },
    node::network::BotanixNetworkPrimitives,
};
use botanix_authority_edh::header_ext::HeaderExt;
use botanix_authority_metrics::AuthorityMetrics;
use botanix_authority_rsp::RandomSource;
use botanix_btc_server_client::{
    BtcServerExtendedApi, BtcServerExtendedClient, Empty, GrpcClientFactory,
    SubscribeToDynafedNotificationsStream,
};
use botanix_btc_wallet::fallback::FallbackBitcoindClient;
use botanix_comet_bft_rpc::{
    Client, CometBftRpcFactory, HttpCometBFTRpcClientFactory,
};
use botanix_configs::federation::AuthorityMultisigConfig;
use botanix_data_parser::{DataParser, SerializationType};
use botanix_storage::{
    MultisigManagerReader, StagedHeaderReader, StagedHeaderWriter,
    WalletStateSyncReader, WalletStateSyncWriter,
};
use futures::{pin_mut, StreamExt};
use reth_network::{frost::manager::ToFrostManager, NetworkHandle};
use reth_primitives::NodePrimitives;
use reth_provider::{
    BlockReaderIdExt, CanonChainTracker, CanonStateSubscriptions,
    StateProviderFactory,
};
use reth_storage_api::NodePrimitivesProvider;
use reth_tasks::TaskExecutor;
use std::{sync::Arc, time::Duration};
use tracing::info;

/// Builder type for configuring and assembling the authority operator for
/// federation/multisig members.
///
/// Collects all dependencies needed to construct a [`FrostTask`] and its
/// supporting background tasks (health monitoring, dynafed notification
/// forwarding, wallet state sync). Only federation members run this.
#[allow(missing_debug_implementations)]
pub struct OperatorBuilder<RDB, BDB, ToFrostMan, Source> {
    /// Reth and Botanix storage backends.
    storage: Storage<RDB, BDB>,
    /// Factory for establishing gRPC connections to the btc-server.
    btc_server_factory: GrpcClientFactory,
    /// P2P network handle for broadcasting and receiving messages.
    network_handle: NetworkHandle<BotanixNetworkPrimitives>,
    /// Handle for sending commands to the FROST signing manager.
    frost_handle: ToFrostMan,
    /// Handle for submitting multisig lifecycle transitions.
    multisig_handle: MultisigSubmitter,
    /// Executor for spawning critical background tasks.
    task_executor: TaskExecutor,
    /// Per-multisig authority configurations .
    multisig_configs: Vec<AuthorityMultisigConfig>,
    /// Factory for creating CometBFT RPC clients.
    cometbft_rpc_factory: HttpCometBFTRpcClientFactory,
    /// Provider of randomness for the remote signing protocol.
    random_source_provider: Source,
    /// Shared authority metrics.
    metrics: Arc<AuthorityMetrics>,
    /// Bitcoin Core RPC client.
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
        + MultisigManagerReader
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
        multisig_handle: MultisigSubmitter,
        task_executor: TaskExecutor,
        multisig_configs: Vec<AuthorityMultisigConfig>,
        cometbft_rpc_factory: HttpCometBFTRpcClientFactory,
        random_source_provider: Source,
        bitcoind_client: Arc<FallbackBitcoindClient>,
    ) -> Self {
        Self {
            storage,
            btc_server_factory,
            network_handle,
            frost_handle,
            multisig_handle,
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
            multisig_handle,
            task_executor,
            multisig_configs,
            cometbft_rpc_factory,
            random_source_provider,
            metrics,
            bitcoind_client,
        } = self;

        let parser = DataParser::default()
            .with_serialization_type(SerializationType::Postcard);

        let btc_server = btc_server_factory
            .build_and_connect()
            .await
            .expect("Failed to build and connect to btc server")
            .into();

        // TODO:
        let wallet_sync = WalletStateSyncEngine::new(
            storage.clone(),
            btc_server.clone(),
            frost_handle.clone(),
            task_executor.clone(),
            2, // TODO
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
            multisig_handle,
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

        frost_task
    }
}
