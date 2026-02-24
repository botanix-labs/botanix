use crate::{
    consensus::{
        comet_bft::abci::{ABCIClientBuilder, ABCIDriverMessage},
        snapshot_manager::{SnapshotManager, SnapshotManagerStateLock},
        utils::{is_poa_epoch, seal_slow},
        Storage,
    },
    node::{
        consensus::BotanixConsensus, evm::config::BotanixEvmConfig, BotanixNode,
    },
};
use alloy_primitives::Address;
use botanix_activation_manager::{ActivationManager, VoteWatcher};
use botanix_authority_edh::header_ext::HeaderExt;
use botanix_authority_metrics::AuthorityMetrics;
use botanix_bitcoin_checkpoint::BitcoinCheckpointsChain;
use botanix_btc_server_client::{
    BtcServerExtendedApi, BtcServerExtendedClient, GrpcClientFactory,
};
use botanix_btc_wallet::fallback::FallbackBitcoindClient;
use botanix_chainspec::BotanixChainSpec;
use botanix_cli_args::state_sync::StateSyncArgs;
use botanix_comet_bft_rpc::HttpCometBFTRpcClientFactory;
use botanix_data_parser::{DataParser, SerializationType};
use botanix_storage::{
    RuntimeTransitionsReadWrite, SnapshotReader, SnapshotWriter,
    StagedHeaderReader, StagedHeaderWriter, WalletStateSyncReader,
    WalletStateSyncWriter,
};
use reth_db::DatabaseEnv;
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
    sync::{Arc, RwLock},
};
use tracing::{info, warn};

/// Builder type for configuring the setup
#[allow(dead_code)]
pub struct AuthorityConsensusBuilder<RDB, BDB> {
    consensus: BotanixConsensus<BotanixChainSpec>,
    storage: Storage<RDB, BDB>,
    activation_manager: ActivationManager<VoteWatcher, Address>,
    is_fed_node: bool,
    bitcoin_checkpoints: Arc<BitcoinCheckpointsChain>,
    task_executor: TaskExecutor,
    cometbft_rpc_factory: HttpCometBFTRpcClientFactory,
    metrics: Arc<AuthorityMetrics>,
    abci_driver_tx: tokio::sync::mpsc::Sender<ABCIDriverMessage>,
    reth_provider_factory:
        ProviderFactory<NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>>,
    state_sync: StateSyncArgs,
    block_fee_recipient_address: Option<alloy_primitives::Address>,
}

/// Errors that can occur when building an authority consensus.
#[derive(Debug)]
pub enum AuthorityConsensusBuilderError {
    InvalidStorage,
    FailedToRecoverAuthorityList,
    FailedToFindSignerIndex,
    FailedToRetrieveEopchHeader,
}

impl<RDB, BDB> AuthorityConsensusBuilder<RDB, BDB>
where
    RDB: BlockReaderIdExt<Header = alloy_consensus::Header>
        + StateProviderFactory
        + Clone
        + CanonChainTracker
        + CanonStateSubscriptions
        + reth_provider::ChainSpecProvider<
            ChainSpec: reth_chainspec::EthereumHardforks,
        > + 'static,
    // TODO: Those bounds can be simplified significantly
    BDB: SnapshotReader
        + SnapshotWriter
        + WalletStateSyncWriter
        + WalletStateSyncReader
        + StagedHeaderReader
        + StagedHeaderWriter
        + RuntimeTransitionsReadWrite
        + Clone
        + 'static,
{
    /// Creates a new builder instance to configure all parts.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        chain_spec: Arc<BotanixChainSpec>,
        reth_provider: RDB,
        activation_manager: ActivationManager<VoteWatcher, Address>,
        is_fed_node: bool,
        bitcoin_checkpoints: Arc<BitcoinCheckpointsChain>,
        task_executor: TaskExecutor,
        btc_network: bitcoin::Network,
        evm_config: BotanixEvmConfig,
        cometbft_rpc_factory: HttpCometBFTRpcClientFactory,
        abci_driver_tx: tokio::sync::mpsc::Sender<ABCIDriverMessage>,
        state_sync: StateSyncArgs,
        reth_provider_factory: ProviderFactory<
            NodeTypesWithDBAdapter<BotanixNode, Arc<DatabaseEnv>>,
        >,
        botanix_provider_factory: BDB,
        block_fee_recipient_address: Option<alloy_primitives::Address>,
        bitcoind_client: Arc<FallbackBitcoindClient>,
    ) -> Result<Self, AuthorityConsensusBuilderError> {
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

        // TODO: Load multisigs according to `MultisigManager`.

        let mut latest_header = reth_provider
            .latest_header()
            .ok()
            .flatten()
            .unwrap_or_else(|| chain_spec.inner().sealed_genesis_header());
        let mut headers = vec![latest_header.clone()];

        // TODO: What exactly is happening here?
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

        // TODO: We cannot assume that the legacy multisig is always active.
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

        // Try to instantiate storage
        let storage = Storage::new(
            btc_network,
            // Aggregate pk to be filled out by the dkg state machine if we are
            // still on genesis block
            agg_pk,
            evm_config,
            chain_spec.clone(),
            bitcoind_client.clone(),
            reth_provider,
            botanix_provider_factory,
        );

        Ok(Self {
            storage,
            activation_manager,
            is_fed_node,
            consensus: BotanixConsensus::new(chain_spec),
            bitcoin_checkpoints,
            task_executor,
            cometbft_rpc_factory,
            metrics: Arc::new(AuthorityMetrics::default()),
            abci_driver_tx,
            reth_provider_factory,
            state_sync,
            block_fee_recipient_address,
        })
    }

    /// Builds and returns the necessary components for the authority consensus,
    /// including the consensus itself, the client used to interact with the
    /// consensus, and the block production task.
    pub async fn build<BtcServerClient>(
        self,
    ) -> (
        Option<ABCIClientBuilder<RDB, BDB>>,
        Option<SnapshotManager<RDB, BDB>>,
        BotanixConsensus<BotanixChainSpec>,
    )
    where
        BtcServerClient: BtcServerExtendedApi + Clone + Send + Sync + 'static,
        BtcServerExtendedClient: Into<BtcServerClient>,
        <<RDB as NodePrimitivesProvider>::Primitives as NodePrimitives>::BlockHeader: HeaderExt,
    {
        let Self {
            consensus,
            storage,
            is_fed_node,
            activation_manager,
            bitcoin_checkpoints,
            task_executor,
            cometbft_rpc_factory,
            metrics,
            abci_driver_tx,
            reth_provider_factory,
            state_sync,
            block_fee_recipient_address,
        } = self;

        let parser = DataParser::default()
            .with_serialization_type(SerializationType::Postcard);

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

        (abci_client_builder, snapshot_manager, consensus)
    }
}
