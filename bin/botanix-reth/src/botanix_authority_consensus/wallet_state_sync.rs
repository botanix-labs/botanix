//! Wallet state sync module
use crate::botanix_authority_consensus::{
    utils::{get_block_pegouts, EpochPegoutsError},
    Storage,
};
use bitcoin::hashes::{sha256::Hash as Sha256Hash, FromSliceError};
use botanix_authority_edh::extra_data_header::ExtraDataHeaderDeserializeError;
use botanix_btc_wallet::bitcoind::BitcoindFactory;
use botanix_data_parser::{
    prost_parser::ProstMessageSerdelizer, DataParser, Error as CompressorError, SerializationType,
};
use botanix_storage::{models::uuid_to_b256, WalletStateSyncReader, WalletStateSyncWriter};
use botanix_btc_server_client::{
    BtcServerExtendedApi, FinalizedPegout, GetFinalizedPegoutIdsResponse, GrpcClientError,
    ResetWalletStateRequest,
};
use btcserverlib::pegout_id::PegoutId;
use once_cell::sync::Lazy;
// use reth_evm::execute::BlockExecutorProvider;
use reth_evm::ConfigureEvm;
use reth_network::frost::{
    manager::{FrostCommand, FrostConfig, ToFrostManager},
    PeerMessageResponse,
};
use alloy_primitives::Bytes;
use reth_provider::{
    BlockReaderIdExt, CanonStateNotification, CanonStateSubscriptions, ProviderError,
};
use reth_tasks::TaskExecutor;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{mpsc::error::SendError, RwLock};
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

const MAX_BLOCK_TS_CUTOFF_DURATION_SECS: u64 = 30 * 24 * 60 * 60 * 3; // 3 months

/// Maximum duration for block timestamp cutoff
/// This is used to determine how far back we should consider finalized pegouts when syncing.
pub static MAX_BLOCK_TS_CUTOFF_DURATION: Lazy<Duration> =
    Lazy::new(|| Duration::from_secs(MAX_BLOCK_TS_CUTOFF_DURATION_SECS));

#[derive(Debug, thiserror::Error)]
/// Wallet state synchronization errors
pub enum WalletStateSyncError {
    #[error("db provider error: {0}")]
    /// Error related to the database provider
    Provider(#[from] ProviderError),
    #[error("deserilaize extra data header : {0}")]
    /// Extra data header deserialize error
    DeserializeExtraDataHeaderError(#[from] ExtraDataHeaderDeserializeError),
    #[error("btc server client error: {0}")]
    /// Btc server client error
    BtcServerClientError(#[from] GrpcClientError),
    #[error("frost manager send error: {0}")]
    /// Frost manager send error
    FrostManagerSendError(#[from] SendError<FrostCommand>),
    #[error("peer never responded with wallet state, timer elapsed")]
    /// Peer wallet state timeout
    PeerWalletStateTimeout,
    #[error("Failed to receive a frost message from a peer {0}")]
    /// Frost recv error
    FrostRecv(tokio::sync::oneshot::error::RecvError),
    #[error("Failed to decompress wallet state data {0}")]
    /// Compressor error
    CompressorError(#[from] CompressorError),
    #[error("UTXO set from peer is not in sync with the latest block, current utxo set merkel root: {0}, latest utxo set merkel root: {1}")]
    /// Utxo set not in sync
    UtxoSetNotInSync(Sha256Hash, Sha256Hash),
    #[error("Failed to convert slide to sha256 hash {0}")]
    /// Sha256 hash error
    Sha256HashError(#[from] FromSliceError),
}
/// Trait for synchronizing wallet state
#[allow(async_fn_in_trait)]
pub trait WalletStateSync {
    /// Synchronizes the wallet state
    async fn sync_wallet_state(&self) -> Result<(), WalletStateSyncError>;
}

type WalletStateSyncResponseCycle = Arc<RwLock<Option<Uuid>>>;
#[derive(Clone)]
/// Engine for synchronizing wallet state
pub struct WalletStateSyncEngine<BF, RDB, BDB, ToFrostMan, BtcServerClient> {
    storage: Storage<BF, RDB, BDB>,
    btc_server: BtcServerClient,
    to_frost_manager: ToFrostMan,
    data_parser: DataParser,
    task_executor: TaskExecutor,
    frost_config: FrostConfig,
    current_response_cycle: WalletStateSyncResponseCycle,
}

impl<BF, RDB, BDB, ToFrostMan, BtcServerClient>
    WalletStateSyncEngine<BF, RDB, BDB, ToFrostMan, BtcServerClient>
where
    BF: BitcoindFactory + Clone + 'static,
    ToFrostMan: ToFrostManager + Sync + Clone + 'static,
    RDB: BlockReaderIdExt + CanonStateSubscriptions + Clone + 'static,
    BDB: WalletStateSyncWriter + WalletStateSyncReader + Clone + 'static,
    BtcServerClient: BtcServerExtendedApi + Clone,
{
    pub(crate) fn new(
        storage: Storage<BF, RDB, BDB>,
        btc_server: BtcServerClient,
        to_frost_manager: ToFrostMan,
        task_executor: TaskExecutor,
        frost_config: FrostConfig,
    ) -> Self {
        let data_parser =
            DataParser::default().with_serialization_type(SerializationType::Postcard);
        Self {
            storage,
            btc_server,
            to_frost_manager,
            data_parser,
            task_executor,
            frost_config,
            current_response_cycle: Default::default(),
        }
    }
}

/// check the L2 existence of the pegouts
async fn hydrate_minimum_superset(
    minimum_superset: HashSet<(u64, Bytes)>,
    client: &impl BlockReaderIdExt,
    btc_network: bitcoin::Network,
) -> Result<HashMap<u64, Vec<(PegoutId, u64)>>, EpochPegoutsError> {
    // Group data by block number
    let mut superset_map: HashMap<u64, Vec<Bytes>> = HashMap::new();
    for (block_num, data) in minimum_superset {
        superset_map.entry(block_num).or_default().push(data);
    }

    // Create futures for each block
    let futures = superset_map.into_iter().map(|(block, data)| async move {
        // Get valid pegout IDs for this block
        let pegouts_result =
            get_block_pegouts(block, client, btc_network, Some(*MAX_BLOCK_TS_CUTOFF_DURATION))
                .await;

        match pegouts_result {
            Ok(pegouts_in_block) => {
                // Filter data to only include valid pegout IDs
                let hydrated_data = data
                    .into_iter()
                    .filter_map(|item| match PegoutId::from_bytes(&item) {
                        Ok(pegout_id) => pegouts_in_block
                            .iter()
                            .find(|(block_pegout_id, _)| *block_pegout_id == pegout_id)
                            .cloned(),
                        Err(_) => None,
                    })
                    .collect::<Vec<_>>();

                Ok((block, hydrated_data))
            }
            Err(e) => Err(e),
        }
    });

    // Execute all futures in parallel
    let results = futures::future::join_all(futures).await;

    // Process results
    let mut hydrated_superset_map = HashMap::new();
    for result in results {
        match result {
            Ok((block, data)) => {
                if !data.is_empty() {
                    hydrated_superset_map.insert(block, data);
                }
            }
            Err(e) => return Err(e),
        }
    }

    Ok(hydrated_superset_map)
}

impl<BF, RDB, BDB, ToFrostMan, BtcServerClient> WalletStateSync
    for WalletStateSyncEngine<BF, RDB, BDB, ToFrostMan, BtcServerClient>
where
    BF: BitcoindFactory + Clone + 'static,
    ToFrostMan: ToFrostManager + Clone + Sync + 'static,
    RDB: BlockReaderIdExt + CanonStateSubscriptions + Clone + 'static,
    BDB: WalletStateSyncWriter + WalletStateSyncReader + Clone + 'static,
    BtcServerClient: BtcServerExtendedApi + Clone,
{
    // Note: this function should not be called unless we are fully synced
    async fn sync_wallet_state(&self) -> Result<(), WalletStateSyncError> {
        Ok(())
    }
}
