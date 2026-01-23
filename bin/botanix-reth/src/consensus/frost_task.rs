use crate::{
    consensus::{
        signing::SigningStateMachine,
        utils::{
            get_pending_pegouts_from_pegout_data,
            get_pending_pegouts_from_staged_pegouts, get_utxos_from_pegin_meta,
            get_utxos_from_staged_pegins, is_poa_epoch, retry_exec,
            validate_psbt_by_ids,
        },
        Storage,
    },
    node::network::BotanixNetworkPrimitives,
};
use alloy_consensus::{BlockHeader, Sealable};
use alloy_primitives::B256;
use bitcoin::consensus::Encodable;
use botanix_authority_edh::header_ext::HeaderExt;
use botanix_authority_metrics::AuthorityMetrics;
use botanix_authority_peg::peg_contract::{PeginMeta, PegoutWithId};
use botanix_authority_rsp::RandomSource;
use botanix_btc_server_client::{
    BtcServerExtendedApi, ConsensusCheckpointRequest, GrpcClientError,
    PendingPegout, SubscribeToDynafedNotificationsStream, Utxo,
};
use botanix_chainspec::BotanixChainSpec;
use botanix_comet_bft_rpc::{
    Client, CometBftRpcFactory, HttpCometBFTRpcClientFactory,
};
use botanix_data_parser::{
    prost_parser::{ProstError, ProstMessageSerdelizer},
    DataParser, Error as DataParserError,
};
use botanix_storage::{
    models::{uuid_to_migration_id, MigrationRecord, MigrationStatus},
    MigrationReader, MigrationWriter, StagedHeaderReader, StagedHeaderWriter,
};
use botanix_types::{MultisigId, LEGACY_MULTISIG_ID};
use btcserverlib::{
    dkg::{
        DkgNotification, DynafedSubscriptionMessage, MigrationEvent,
        MigrationNotification,
    },
    wallet::psbt::frost_id_from_bytes,
};
use futures::{pin_mut, StreamExt};
use reth_network::{
    frost::{
        manager::{
            authority_index_to_frost_identifier, FrostCommand, FrostConfig,
            PeerData, ToFrostManager,
        },
        DkgResponse, FrostPeerCommand, PeerMessageResponse,
        SigningEventResponseType, SigningResponse, WalletStateResponse,
    },
    NetworkHandle,
};
use reth_primitives::{Header, NodePrimitives};
use reth_provider::{
    BlockReaderIdExt, CanonStateNotification, CanonStateSubscriptions,
    StateProviderFactory,
};
use reth_revm::primitives::FixedBytes;
use reth_storage_api::NodePrimitivesProvider;
use std::{
    collections::{BTreeMap, HashMap},
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use tendermint_rpc::client::HttpClient;
use tokio::sync::mpsc::{self, error::SendError};
use tracing::{error, info, trace, warn};
use uuid::Uuid;

// TODO: @rwlock Combine with FrostTaskError?
#[derive(Debug, thiserror::Error)]
/// Errors that can occur during synchronization.
pub(crate) enum SyncError {
    #[error("tendermint error")]
    /// Error related to Tendermint.
    Tendermint(#[from] tendermint::Error),
    /// Error related to Tendermint RPC.
    #[error("tendermint rpc error")]
    TendermintRpc(#[from] tendermint_rpc::Error),
}

// TODO: @rwlock Combine with FrostTaskError?
#[derive(Debug, thiserror::Error)]
pub(crate) enum FinalizedPegoutIdsSyncSerializationError {
    #[error("Received a grpc client error {0}")]
    Grpc(#[from] GrpcClientError),
    #[error("prost error {0}")]
    Prost(#[from] ProstError),
    #[error("data parser error {0}")]
    DataParser(#[from] DataParserError),
}

// Migration types are now in botanix_storage::models

#[allow(dead_code)]
pub struct FrostTask<RDB, BDB, ToFrostMan, Source, BtcServerClient> {
    /// Network Handler
    pub(crate) network_handle: NetworkHandle<BotanixNetworkPrimitives>,
    /// Frost network Handler
    pub(crate) frost_handle: ToFrostMan,
    /// Frost configuration
    pub(crate) frost_config: FrostConfig,
    /// signing state machine
    pub(crate) signing_state_machine:
        SigningStateMachine<ToFrostMan, Source, BtcServerClient>,
    /// Shared storage to insert aggregate public key
    pub(crate) storage: Storage<RDB, BDB>,
    /// A handle to the `DkgRunnerTask` task. This is only `Some` if no
    /// aggregate public key is available, and the `start_task` method has
    /// hence started the DKG process.
    dkg_tasks: Option<BTreeMap<MultisigId, mpsc::Sender<DkgResponse>>>,
    /// Pre-configured data-parser
    compressor: DataParser,
    /// btc server client
    btc_server: BtcServerClient,
    /// Indicates whether staged headers should be checked by the Frost task
    /// and submitted to the btc-server to initiate a checkpoint. This is
    /// `true` in two scenarios:
    /// 1. On initial startup.
    /// 2. The connection to the btc-server has been interrupted.
    check_staged_headers: bool,
    /// Authority Metrics
    metrics: Arc<AuthorityMetrics>,
    /// cometbft light client provider
    cbft_rpc_provider: HttpClient,
    /// Dynafed frost notifications subscriber
    dynafed_frost_notifications_tx:
        tokio::sync::broadcast::Sender<SubscribeToDynafedNotificationsStream>,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum FrostTaskError {
    #[error("Unable to get all connected peers {0}")]
    UnableToGetAllConnectedPeers(#[from] SendError<FrostCommand>),
}

impl<RDB, BDB, ToFrostMan, Source, BtcServerClient>
    FrostTask<RDB, BDB, ToFrostMan, Source, BtcServerClient>
where
    ToFrostMan: 'static + Send + Sync + ToFrostManager + Clone,
    RDB: BlockReaderIdExt + StateProviderFactory + CanonStateSubscriptions + Clone + 'static,
    <<RDB as NodePrimitivesProvider>::Primitives as NodePrimitives>::BlockHeader:
        HeaderExt + Sealable,
    BDB: StagedHeaderReader + StagedHeaderWriter + MigrationReader + MigrationWriter + Clone + 'static,
    Source: RandomSource,
    BtcServerClient: BtcServerExtendedApi + Clone,
{
    /// Creates a new instance of the task
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        chain_spec: Arc<BotanixChainSpec>,
        btc_server: BtcServerClient,
        network_handle: NetworkHandle<BotanixNetworkPrimitives>,
        frost_handle: ToFrostMan,
        config: FrostConfig,
        storage: Storage<RDB, BDB>,
        compressor: DataParser,
        random_source_provider: Source,
        metrics: Arc<AuthorityMetrics>,
        cometbft_rpc_factory: HttpCometBFTRpcClientFactory,
        dynafed_frost_notifications_tx: tokio::sync::broadcast::Sender<SubscribeToDynafedNotificationsStream>,
    ) -> Self {
        info!(target: "consensus::authority::frost_task::new", "Frost authority index: {}/{}", config.authority_index, config.authorities.len() - 1);

        let signing_state_machine = SigningStateMachine::new(
            chain_spec,
            btc_server.clone(),
            frost_handle.clone(),
            config.clone(),
            random_source_provider,
            metrics.clone(),
        );

        let cbft_rpc_provider =
            cometbft_rpc_factory.build_and_connect().expect("light client to connect");

        Self {
            network_handle,
            frost_handle,
            frost_config: config,
            signing_state_machine,
            storage,
            btc_server,
            check_staged_headers: true,
            dkg_tasks: None,
            compressor,
            metrics,
            cbft_rpc_provider,
            dynafed_frost_notifications_tx,
        }
    }

    async fn is_syncing(&self) -> Result<bool, SyncError> {
        let status = self.cbft_rpc_provider.status().await?;
        Ok(status.sync_info.catching_up)
    }

    async fn send_serialized_compressed_finalized_pegout_ids(
        &mut self,
        chunk_size: u64,
        peer_data: &PeerData,
        wallet_state_response: &WalletStateResponse,
    ) -> Result<(), FinalizedPegoutIdsSyncSerializationError> {
        // create the request
        let request = botanix_btc_server_client::GetFinalizedPegoutIdsRequest { chunk_size };

        // call the streaming RPC method
        let response = self.btc_server.get_finalized_pegout_ids(request).await?;
        pin_mut!(response);

        let mut received_healthy_chunks = 0;
        let mut total_expected_chunks;
        let mut is_final_chunk_received = false;

        // get the stream from the response
        while let Some(item) = response.next().await {
            match item {
                Ok(prost_serialized_pegout_ids) => {
                    total_expected_chunks = prost_serialized_pegout_ids.total_chunks;
                    if prost_serialized_pegout_ids.is_final {
                        is_final_chunk_received = true;
                    }
                    if prost_serialized_pegout_ids.data.is_empty() {
                        warn!(target: "consensus::authority::frost_task::send_serialized_compressed_finalized_pegout_ids", "Received empty finalized pegout ids from btc server");
                        continue;
                    }

                    // serialize the prost message
                    let prost_message_wrapper = ProstMessageSerdelizer(prost_serialized_pegout_ids);
                    let prost_serialized = prost_message_wrapper.serialize().map_err(|e| {
                        error!(target: "consensus::authority::frost_task::send_serialized_compressed_finalized_pegout_ids", "Got serializer error {:?}", e);
                        FinalizedPegoutIdsSyncSerializationError::Prost(e)
                    })?;

                    // now compress the prost message
                    let prost_serialized_compressed = self.compressor.compress(&prost_serialized).await.map_err(|e| {
                        error!(target: "consensus::authority::frost_task::send_serialized_compressed_finalized_pegout_ids", "Got compressor error {:?}", e);
                        FinalizedPegoutIdsSyncSerializationError::DataParser(e)
                    })?;
                    received_healthy_chunks += 1;

                    let mut wallet_state_response = wallet_state_response.clone();
                    wallet_state_response.finalized_pegout_ids = prost_serialized_compressed;

                    trace!(target: "consensus::authority::frost_task::start_task", "Sending wallet state to peer {:?}", peer_data.peer_id);
                    if let Err(e) = peer_data.peer_commands_tx.send(FrostPeerCommand::PeerMessage(
                        PeerMessageResponse::WalletState(wallet_state_response),
                    )) {
                        error!(target: "consensus::authority::frost_task::start_task", "Error sending wallet state message to peer {:?}: {:?}",  peer_data.peer_id, e);
                        continue;
                    }
                }
                Err(e) => {
                    error!(target: "consensus::authority::frost_task::send_serialized_compressed_finalized_pegout_ids", "Got grpc error {:?}", e);
                    continue;
                }
            }

            if (received_healthy_chunks == total_expected_chunks) && is_final_chunk_received {
                trace!(target: "consensus::authority::frost_task::send_serialized_compressed_finalized_pegout_ids", "Received all chunks");
            } else {
                trace!(target: "consensus::authority::frost_task::send_serialized_compressed_finalized_pegout_ids", "Received {} out of {} chunks", received_healthy_chunks, total_expected_chunks);
            }
        }
        Ok(())
    }

    fn has_wallet_state(response: &WalletStateResponse) -> bool {
        !response.finalized_pegout_ids.is_empty()
    }

    /// Handles the canon state commit notification by submitting the pegins and
    /// pegouts to the btc-server and initiating a checkpoint and signing
    /// session, if necessary.
    async fn handle_canon_state_commit<H>(
        &mut self,
        // Note: `header_hash` is the hash of the header that is being
        // committed. We pass this on such that calling `header.hash_slow()`
        // can be avoided. Ideally, this is already precomputed when
        // passed on.
        header_hash: B256,
        header: &H,
        pegins: Vec<Utxo>,
        pending_pegouts: Vec<PendingPegout>,
    ) where
        H: BlockHeader + Sealable + HeaderExt,
    {
        debug_assert_eq!(header_hash, header.hash_slow());

        info!(
            target: "consensus::authority::frost_task::handle_canon_state_commit",
            "Handling canon state commit for block number {:?}", header.number()
        );

        let edh = match header.deserialize_extra_data_header() {
            Ok(edh) => edh,
            Err(e) => {
                error!(
                    target: "consensus::authority::frost_task::handle_canon_state_commit",
                    "Error deserializing extra data header: {}", e
                );

                return;
            }
        };

        let cp_block_hash = edh.bitcoin_block_hash;
        let mut block_hash_writer = vec![];

        if let Err(e) = cp_block_hash.consensus_encode(&mut block_hash_writer) {
            error!(
                target: "consensus::authority::frost_task::handle_canon_state_commit",
                "Error encoding checkpoint block hash: {}", e
            );

            return;
        }

        let btc_server_capture = self.btc_server.clone();
        let block_hash_writer = block_hash_writer.clone();
        let pegins = pegins.clone();
        let pending_pegouts = pending_pegouts.clone();

        let fut = move || {
            let mut btc_server = btc_server_capture.clone();
            let block_hash = block_hash_writer.clone();
            let pegins_data = pegins.clone();
            let pending_data = pending_pegouts.clone();

            async move {
                btc_server
                    .new_consensus_checkpoint(ConsensusCheckpointRequest {
                        checkpoint_block_hash: block_hash,
                        pegins: pegins_data,
                        pending_pegouts: pending_data,
                    })
                    .await
            }
        };

        // (Re-)try initiating a checkpoint on the btc-server.
        match retry_exec("new_consensus_checkpoint", fut, 3, Duration::from_secs(2)).await {
            Ok(_) => {
                info!(
                    target: "consensus::authority::frost_task::handle_canon_state_commit",
                    "Sent checkpoint to btc server"
                );

                // Remove staged entries for this block hash; it's now the
                // responsibility of the btc-server to keep track of the pegins
                // and pegouts.
                let existed = self
                    .storage
                    .botanix_database_factory
                    .remove_staged_header(header_hash)
                    .expect("to remove staged header");

                debug_assert!(existed, "Staged header should exist for the given header hash");
            }
            Err(err) => {
                error!(
                    target: "consensus::authority::frost_task::handle_canon_state_commit",
                    "Failed to send checkpoint to btc server: {}", err
                );

                // Indicate to the Frost task that we should check staged
                // headers on the next iteration. Ideally, the connection to the
                // btc-server is restored at some point later and the staged
                // headers can be processed.
                self.check_staged_headers = true;

                return;
            }
        }

        // Check if this is an epoch block and if we are the coordinator. If
        // yes, initiate signing session.
        if !is_poa_epoch(header.number(), self.storage.chain_spec.epoch_length) {
            return;
        }

        if !self.signing_state_machine.is_coordinator() {
            info!(
                target: "consensus::authority::frost_task::handle_canon_state_commit",
                "Received canon state notification during epoch block but we're not the coordinator"
            );

            return;
        }

        // Create psbt and send init signing message.
        let psbt_payload = match crate::consensus::utils::get_psbt(
            &mut self.btc_server,
            &header_hash,
            cp_block_hash,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                error!(
                    target: "consensus::authority::frost_task::handle_canon_state_commit",
                    "Failed to get psbt {:?}", e
                );

                return;
            }
        };

        // Validate psbt.
        let psbt = match bitcoin::Psbt::deserialize(psbt_payload.psbt.as_slice()) {
            Ok(psbt) => psbt,
            Err(e) => {
                error!(
                    target: "consensus::authority::frost_task::handle_canon_state_commit",
                    "Error deserializing psbt {:?}", e
                );

                return;
            }
        };

        if let Err(e) =
            validate_psbt_by_ids(&self.storage.reth_database, self.storage.btc_network, &psbt).await
        {
            error!(
                target: "consensus::authority::frost_task::handle_canon_state_commit",
                "Error validating psbt {:?}", e
            );

            return;
        };

        info!(
            target: "consensus::authority::frost_task::handle_canon_state_commit",
            "Validated psbt successfully"
        );

        // Initiate signing session.
        if let Err(e) =
            self.signing_state_machine.initiate_signing_session(header_hash, psbt_payload.psbt).await
        {
            error!(
                target: "consensus::authority::frost_task::handle_canon_state_commit",
                "Error starting new signing session {:?}", e
            );

            return;
        };

        info!(
            target: "consensus::authority::frost_task::handle_canon_state_commit",
            "Started new signing session successfully"
        );
    }

    pub async fn start_task(&mut self, mut abci_started_rx: tokio::sync::oneshot::Receiver<()>) {
        // before we start get a proper event receiver
        let (peer_messages_tx, peer_messages_rx) = tokio::sync::oneshot::channel();

        let mut peer_messages_rx = match self
            .frost_handle
            .send_command(FrostCommand::GetPeerMessagesStream(peer_messages_tx))
        {
            Ok(_) => {
                // only await on the receiver if the send was successful
                match peer_messages_rx.await {
                    Ok(rx) => rx,
                    Err(e) => {
                        error!(target: "consensus::authority::frost_task::start_task", "Error getting receiver handle = {:?}", e);
                        panic!("Error getting receiver handle. Error - {e:?}");
                    }
                }
            }
            Err(e) => {
                error!(target: "consensus::authority::frost_task::start_task", "Failed to send GetPeerMessagesStream frost command {}", e);
                panic!("Failed to send GetPeerMessagesStream frost command - {e:?}");
            }
        };

        // Calling get pk
        // Attempt to get the aggregate public key and store in storage
        if let Ok(public_key) = self
            .btc_server
            .get_public_key(botanix_btc_server_client::GetPublicKeyRequest {
                multisig_id: *LEGACY_MULTISIG_ID
            })
            .await
        {
            info!(target: "consensus::authority::frost_task::start_task", " received aggregate public key from dkg state machine {:?}", public_key);
            if let Ok(secp_pk) = secp256k1::PublicKey::from_slice(
                hex::decode(public_key.publickey)
                    .expect("invalid aggregated public key")
                    .as_slice(),
            ) {
                let mut storage = self.storage.inner.write().await;
                storage.aggregate_public_key = Some(BTreeMap::from([(LEGACY_MULTISIG_ID, secp_pk)]));

                drop(storage);
            } else {
                warn!(
                    target: "consensus::authority::frost_task::start_task", "converting public key to secp256k1 public key"
                );
            }
        } else {
            warn!(target: "consensus::authority::frost_task::start_task", "No public key found, proceeding with DKG");

            // Start the dkg state machine task runner.
            let tx = DkgRunnerTask::new(
                self.frost_handle.clone(),
                self.frost_config.authorities.as_ref(),
                self.storage.clone(),
                self.btc_server.clone(),
                Arc::clone(&self.metrics),
                LEGACY_MULTISIG_ID,
            );
            if let Some(tasks) = self.dkg_tasks.as_mut() {
                tasks.insert(LEGACY_MULTISIG_ID, tx.clone());
            } else {
                let mut tasks = BTreeMap::new();
                tasks.insert(LEGACY_MULTISIG_ID, tx.clone());
                self.dkg_tasks = Some(tasks);
            }

            info!(target: "consensus::authority::frost_task::start_task", "DKG runner task started...");
        }
        let mut canon_state_notifs = self.storage.reth_database.subscribe_to_canonical_state();

        let mut abci_started = false;
        let mut dynafed_frost_notifications_rx = self.dynafed_frost_notifications_tx.subscribe();
        let frost_handle_clone = self.frost_handle.clone();
        let frost_config_clone = self.frost_config.clone();
        let storage_clone = self.storage.clone();
        let btc_server_clone = self.btc_server.clone();
        let metrics_clone = Arc::clone(&self.metrics);

        loop {
            // check if abci has started
            if abci_started_rx.try_recv().is_ok() {
                abci_started = true;
            }
            if abci_started {
                // get sync status
                match self.is_syncing().await {
                    Ok(is_syncing) => {
                        self.storage.inner.write().await.is_block_syncing = is_syncing;
                        if is_syncing {
                            info!(target: "consensus::authority::frost_task::start_task", "Node is syncing, pausing frost task...");
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            continue;
                        }
                    }
                    Err(e) => {
                        warn!(target: "consensus::authority::frost_task::start_task", "Error getting block sync status {:?}", e);
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }
                }
            }

            // We check the local database for any staged headers; a presence of
            // staged headers indicates that the pegins and pegouts of a
            // specific block have been correctly extracted during finalization,
            // but the checkpoint has not yet been created on the btc-server.
            //
            // This can happen if the connection to the btc-server has been
            // interrupted while block production is continuing.
            if self.check_staged_headers {
                let mut staged_headers = self
                    .storage
                    .botanix_database_factory
                    .get_staged_headers()
                    .expect("to get staged headers");

                if staged_headers.is_empty() {
                    info!(target: "consensus::authority::frost_task::start_task", "No staged headers found, proceeding with frost task");
                } else {
                    warn!(target: "consensus::authority::frost_task::start_task", "Found {} staged headers, proceeding with btc-server checkpoint reconstruction", staged_headers.len());
                }

                // Sort staged headers by block number in ascending order.
                staged_headers.sort_by(|(_, a), (_, b)| a.header.number.cmp(&b.header.number));

                // NOTE: This flag might be overridden by the
                // `handle_canon_state_commit` method.
                self.check_staged_headers = false;

                for (header_hash, entry) in staged_headers {
                    let header = entry.header;

                    let pegins = get_utxos_from_staged_pegins(entry.pegins);
                    let pegouts =
                        get_pending_pegouts_from_staged_pegouts(entry.pegouts, header.timestamp);

                    self.handle_canon_state_commit(header_hash, &header, pegins, pegouts).await;
                }
            }

            // Receive DKG frost notifications
            while let Ok(payload) = dynafed_frost_notifications_rx.try_recv() {
                info!(target: "consensus::authority::frost_task::start_task", "Received dynafed frost notification from btc-server: {:?}", payload);
                let dynafed_sub_message = match payload.notification {
                    Some(botanix_btc_server_client::subscribe_to_dynafed_notifications_stream::Notification::Dkg(dkg)) => {
                        let dkg_notification = match btcserverlib::rpc::DkgEvent::try_from(dkg.event) {
                            Ok(btcserverlib::rpc::DkgEvent::Unspecified) => {
                                warn!(target: "consensus::authority::frost_task::start_task", "Unspecified DKG event received");
                                continue;
                            }
                            Ok(btcserverlib::rpc::DkgEvent::DkgStart) => DkgNotification::Start {
                                multisig_id: dkg.multisig_id.into(),
                            },
                            Ok(btcserverlib::rpc::DkgEvent::DkgRestart) => DkgNotification::Restart {
                                multisig_id: dkg.multisig_id.into(),
                            },
                            Ok(btcserverlib::rpc::DkgEvent::DkgAbort) => DkgNotification::Abort {
                                multisig_id: dkg.multisig_id.into(),
                            },
                            Err(e) => {
                                error!(target: "consensus::authority::frost_task::start_task", "Invalid DKG event value {}: {:?}", dkg.event, e);
                                continue;
                            }
                        };
                        DynafedSubscriptionMessage::Dkg(dkg_notification)
                    }
                    Some(botanix_btc_server_client::subscribe_to_dynafed_notifications_stream::Notification::Migration(migration)) => {
                        let event = match btcserverlib::rpc::MigrationEvent::try_from(migration.event) {
                            Ok(btcserverlib::rpc::MigrationEvent::Unspecified) => {
                                warn!(target: "consensus::authority::frost_task::start_task", "Unspecified migration event received");
                                continue;
                            }
                            Ok(btcserverlib::rpc::MigrationEvent::MigrationStart) => MigrationEvent::Start,
                            Ok(btcserverlib::rpc::MigrationEvent::MigrationEnd) => MigrationEvent::End,
                            Ok(btcserverlib::rpc::MigrationEvent::MigrationAbort) => MigrationEvent::Abort,
                            Err(e) => {
                                error!(target: "consensus::authority::frost_task::start_task", "Invalid migration event value {}: {:?}", migration.event, e);
                                continue;
                            }
                        };

                        let migration_id = match Uuid::parse_str(&migration.migration_id) {
                            Ok(id) => id,
                            Err(e) => {
                                error!(target: "consensus::authority::frost_task::start_task", "Invalid migration UUID: {:?}", e);
                                continue;
                            }
                        };

                        DynafedSubscriptionMessage::Migration(MigrationNotification {
                            event,
                            multisig_id_from: migration.multisig_id_from.into(),
                            multisig_id_to: migration.multisig_id_to.into(),
                            migration_id,
                        })
                    }
                    None => {
                        warn!(target: "consensus::authority::frost_task::start_task", "Received dynafed frost notification with no payload");
                        continue;
                    }
                };

                match dynafed_sub_message {
                    DynafedSubscriptionMessage::Dkg(notification) => {
                        let multisig_id = notification.multisig_id();
                        info!(target: "consensus::authority::frost_task::start_task", "Handling DKG notification for multisig id {}", multisig_id);

                        match notification {
                            btcserverlib::dkg::DkgNotification::Start { multisig_id } => {
                                info!(target: "consensus::authority::frost_task::start_task", "Starting DKG for multisig id {}", multisig_id);
                                // The returned tx needs to be stored with a mapping to the multisig_id
                                if let Some(tasks) = self.dkg_tasks.as_mut() {
                                    if tasks.contains_key(&multisig_id) {
                                        warn!(target: "consensus::authority::frost_task::start_task", "DKG task for multisig id {} already exists, skipping...", multisig_id);
                                        continue;
                                    }
                                    // Start the dkg state machine task runner for that multisig id
                                    let tx = DkgRunnerTask::new(
                                        frost_handle_clone.clone(),
                                        frost_config_clone.authorities.as_ref(),
                                        storage_clone.clone(),
                                        btc_server_clone.clone(),
                                        Arc::clone(&metrics_clone),
                                        multisig_id,
                                    );
                                    tasks.insert(multisig_id, tx.clone());
                                } else {
                                    let mut tasks = BTreeMap::new();
                                    // Start the dkg state machine task runner for that multisig id
                                    let tx = DkgRunnerTask::new(
                                        frost_handle_clone.clone(),
                                        frost_config_clone.authorities.as_ref(),
                                        storage_clone.clone(),
                                        btc_server_clone.clone(),
                                        Arc::clone(&metrics_clone),
                                        multisig_id,
                                    );
                                    tasks.insert(multisig_id, tx.clone());
                                    self.dkg_tasks = Some(tasks);
                                }
                            }
                            btcserverlib::dkg::DkgNotification::Restart { multisig_id } => {
                                if let Some(tasks) = self.dkg_tasks.as_mut() {
                                    if !tasks.contains_key(&multisig_id) {
                                        warn!(target: "consensus::authority::frost_task::start_task", "DKG task for multisig id {} cannot be restarted as it does not exist, skipping...", multisig_id);
                                        continue;
                                    }
                                    tasks.get_mut(&multisig_id).and_then(|v| {
                                        let tx = DkgRunnerTask::new(
                                            frost_handle_clone.clone(),
                                            frost_config_clone.authorities.as_ref(),
                                            storage_clone.clone(),
                                            btc_server_clone.clone(),
                                            Arc::clone(&metrics_clone),
                                            multisig_id,
                                        );
                                        // replace the old tx with the new one
                                        *v = tx;
                                        Some(())
                                    });
                                } else {
                                    warn!(target: "consensus::authority::frost_task::start_task", "DKG task for multisig id {} cannot be restarted as no tasks exist, skipping...", multisig_id);
                                    continue;
                                }
                            }
                            // TODO: probably need to send some signal to the other nodes to abort as well. we also send a message via the NDD to the other nodes to abort their DKG tasks for this multisig id.
                            btcserverlib::dkg::DkgNotification::Abort { multisig_id } => {
                                if let Some(tasks) = self.dkg_tasks.as_mut() {
                                    if !tasks.contains_key(&multisig_id) {
                                        warn!(target: "consensus::authority::frost_task::start_task", "DKG task for multisig id {} cannot be aborted as it does not exist, skipping...", multisig_id);
                                        continue;
                                    }
                                    tasks.remove(&multisig_id);
                                } else {
                                    warn!(target: "consensus::authority::frost_task::start_task", "DKG task for multisig id {} cannot be aborted as no tasks exist, skipping...", multisig_id);
                                    continue;
                                }
                            }
                        }
                    }
                    DynafedSubscriptionMessage::Migration(notification) => {
                        info!(target: "consensus::authority::frost_task::start_task", "Handling Migration notification: {:?}", notification);

                        match notification.event {
                            MigrationEvent::Start => {
                                let migration_id = uuid_to_migration_id(notification.migration_id);

                                // Check if migration already exists in database
                                match self.storage.botanix_database_factory.get_migration(migration_id) {
                                    Ok(Some(_)) => {
                                        warn!(target: "consensus::authority::frost_task::start_task", "Migration with uuid {} already exists, skipping...", notification.migration_id);
                                        continue;
                                    }
                                    Ok(None) => {
                                        // Migration doesn't exist, proceed
                                    }
                                    Err(e) => {
                                        error!(target: "consensus::authority::frost_task::start_task", "Error checking migration in database: {:?}", e);
                                        continue;
                                    }
                                }

                                // Store migration with STARTED status
                                let record = MigrationRecord::new(
                                    migration_id,
                                    *notification.multisig_id_from,
                                    *notification.multisig_id_to,
                                    MigrationStatus::Started,
                                );
                                if let Err(e) = self.storage.botanix_database_factory.store_migration(&record) {
                                    error!(target: "consensus::authority::frost_task::start_task", "Failed to store migration in database: {:?}", e);
                                    continue;
                                }

                                // Check if DKG tasks are already running for these multisig IDs
                                if let Some(tasks) = self.dkg_tasks.as_ref() {
                                    if tasks.contains_key(&notification.multisig_id_from) {
                                        error!(target: "consensus::authority::frost_task::start_task", "DKG task for migration source multisig {} is already running, aborting migration...", notification.multisig_id_from);
                                        // Send abort notification to btc-server
                                        let mut btc_server = btc_server_clone.clone();
                                        match btc_server.abort_migration(
                                            botanix_btc_server_client::AbortMigrationRequest {
                                                migration_id: notification.migration_id.to_string(),
                                            }
                                        ).await {
                                            Ok(_) => {
                                                if let Err(e) = self
                                                    .storage
                                                    .botanix_database_factory
                                                    .remove_migration(migration_id)
                                                {
                                                    error!(target: "consensus::authority::frost_task::start_task", "Failed to remove migration from database: {:?}", e);
                                                }
                                            }
                                            Err(e) => {
                                                error!(target: "consensus::authority::frost_task::start_task", "Failed to abort migration on btc-server: {:?}", e);
                                                // Keep local migration record for retry/consistency.
                                            }
                                        }
                                        continue;
                                    }
                                    if tasks.contains_key(&notification.multisig_id_to) {
                                        error!(target: "consensus::authority::frost_task::start_task", "DKG task for migration target multisig {} is already running, aborting migration...", notification.multisig_id_to);
                                        // Send abort notification to btc-server
                                        let mut btc_server = btc_server_clone.clone();
                                        match btc_server.abort_migration(
                                            botanix_btc_server_client::AbortMigrationRequest {
                                                migration_id: notification.migration_id.to_string(),
                                            }
                                        ).await {
                                            Ok(_) => {
                                                if let Err(e) = self
                                                    .storage
                                                    .botanix_database_factory
                                                    .remove_migration(migration_id)
                                                {
                                                    error!(target: "consensus::authority::frost_task::start_task", "Failed to remove migration from database: {:?}", e);
                                                }
                                            }
                                            Err(e) => {
                                                error!(target: "consensus::authority::frost_task::start_task", "Failed to abort migration on btc-server: {:?}", e);
                                                // Keep local migration record for retry/consistency.
                                            }
                                        }
                                        continue;
                                    }
                                }

                                // Start DKG for the target (new) multisig
                                info!(target: "consensus::authority::frost_task::start_task", "Starting DKG for migration target multisig {}", notification.multisig_id_to);

                                let tx = DkgRunnerTask::new(
                                    frost_handle_clone.clone(),
                                    frost_config_clone.authorities.as_ref(),
                                    storage_clone.clone(),
                                    btc_server_clone.clone(),
                                    Arc::clone(&metrics_clone),
                                    notification.multisig_id_to,
                                );

                                if let Some(tasks) = self.dkg_tasks.as_mut() {
                                    tasks.insert(notification.multisig_id_to, tx);
                                } else {
                                    let mut tasks = BTreeMap::new();
                                    tasks.insert(notification.multisig_id_to, tx);
                                    self.dkg_tasks = Some(tasks);
                                }

                                // Update migration status to RUNNING in database
                                if let Err(e) = self.storage.botanix_database_factory.update_migration_status(migration_id, MigrationStatus::Running) {
                                    error!(target: "consensus::authority::frost_task::start_task", "Failed to update migration status in database: {:?}", e);
                                } else {
                                    info!(target: "consensus::authority::frost_task::start_task", "Migration {} status updated to RUNNING", notification.migration_id);
                                }
                            }
                            MigrationEvent::End => {
                                info!(target: "consensus::authority::frost_task::start_task", "Ending migration {}", notification.migration_id);
                                let migration_id = uuid_to_migration_id(notification.migration_id);

                                // Verify the new multisig has an aggregate public key (is operational)
                                let has_new_multisig_key = {
                                    let storage = self.storage.inner.read().await;
                                    storage.aggregate_public_key
                                        .as_ref()
                                        .map(|keys| keys.contains_key(&notification.multisig_id_to))
                                        .unwrap_or(false)
                                };

                                if !has_new_multisig_key {
                                    warn!(target: "consensus::authority::frost_task::start_task",
                                        "New multisig {} does not have an aggregate public key yet, migration {} may not be fully operational",
                                        notification.multisig_id_to, notification.migration_id);
                                    continue;
                                }

                                // Check if migration exists in database
                                match self.storage.botanix_database_factory.get_migration(migration_id) {
                                    Ok(Some(_)) => {
                                        // Update migration status to FINISHED
                                        if let Err(e) = self.storage.botanix_database_factory.update_migration_status(migration_id, MigrationStatus::Finished) {
                                            error!(target: "consensus::authority::frost_task::start_task", "Failed to update migration status in database: {:?}", e);
                                        } else {
                                            info!(target: "consensus::authority::frost_task::start_task", "Migration {} status updated to FINISHED", notification.migration_id);
                                        }

                                        // Clean up the old (source) multisig DKG task if it exists
                                        if let Some(tasks) = self.dkg_tasks.as_mut() {
                                            if tasks.remove(&notification.multisig_id_from).is_some() {
                                                info!(target: "consensus::authority::frost_task::start_task", "Removed DKG task for old multisig {}", notification.multisig_id_from);
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        warn!(target: "consensus::authority::frost_task::start_task", "Migration {} not found when trying to end it", notification.migration_id);
                                    }
                                    Err(e) => {
                                        error!(target: "consensus::authority::frost_task::start_task", "Error getting migration from database: {:?}", e);
                                    }
                                }
                            }
                            MigrationEvent::Abort => {
                                info!(target: "consensus::authority::frost_task::start_task", "Aborting migration {}", notification.migration_id);
                                let migration_id = uuid_to_migration_id(notification.migration_id);

                                // Remove migration from database
                                match self.storage.botanix_database_factory.remove_migration(migration_id) {
                                    Ok(true) => {
                                        info!(target: "consensus::authority::frost_task::start_task", "Removed migration {} from database", notification.migration_id);
                                    }
                                    Ok(false) => {
                                        warn!(target: "consensus::authority::frost_task::start_task", "Migration {} not found when trying to abort", notification.migration_id);
                                    }
                                    Err(e) => {
                                        error!(target: "consensus::authority::frost_task::start_task", "Error removing migration from database: {:?}", e);
                                    }
                                }

                                // Clean up DKG task for target multisig (if it was started)
                                if let Some(tasks) = self.dkg_tasks.as_mut() {
                                    if tasks.remove(&notification.multisig_id_to).is_some() {
                                        info!(target: "consensus::authority::frost_task::start_task", "Removed DKG task for target multisig {} during abort", notification.multisig_id_to);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Receive canon state notifications
            while let Ok(notification) = canon_state_notifs.try_recv() {
                info!(target: "consensus::authority::frost_task::start_task", "canon state notification received for block number {:?}", notification.tip().number());
                match notification {
                    CanonStateNotification::Commit { new, pegins, pegouts } => {
                        let tip = new.tip();
                        let header_hash = tip.hash();
                        let header = tip.header();

                        // Read aggregate public keys once for lookups
                        let Some(aggregate_public_keys) = self
                            .storage
                            .inner
                            .read()
                            .await
                            .aggregate_public_key
                            .clone()
                        else {
                            // Same pattern as handle_canon_state_commit btc-server failure
                            error!(
                                target: "consensus::authority::frost_task::start_task",
                                "no aggregate public keys found"
                            );

                            // Since we are skipping the current block, we need to check for staged headers on the next iteration.
                            self.check_staged_headers = true;

                            continue;
                        };

                        // Convert pegins into correct format
                        let pegins = pegins.as_ref().map_or_else(Vec::new, |pegins| {
                            let deserialized = pegins
                                .iter()
                                .filter_map(|bytes| match PeginMeta::deserialize(bytes) {
                                    Ok((pegin, _)) => Some(pegin),
                                    Err(e) => {
                                        error!("Failed to deserialize pegin: {:?}", e);
                                        None
                                    }
                                }).collect::<Vec<_>>();
                                get_utxos_from_pegin_meta(deserialized.as_slice(), &aggregate_public_keys)
                            });

                        // Convert pegouts into correct format
                        let pending_pegouts = pegouts.as_ref().map_or_else(Vec::new, |pegouts| {
                            let deserialized = pegouts
                                .iter()
                                .filter_map(|bytes| match PegoutWithId::deserialize(bytes) {
                                    Ok(pegout) => Some(pegout),
                                    Err(e) => {
                                        error!("Failed to deserialize pegout: {:?}", e);
                                        None
                                    }
                                })
                                .collect::<Vec<_>>();
                            get_pending_pegouts_from_pegout_data(
                                &deserialized,
                                tip.number(),
                                tip.header().timestamp(),
                            )
                        });

                        self.handle_canon_state_commit(
                            header_hash,
                            header,
                            pegins,
                            pending_pegouts,
                        )
                        .await;
                    }
                    _ => {
                        // Ignore other notifications
                    }
                }
            }

            // receive over a channel message from other peers and update our
            // state machine
            while let Ok(message_context) = peer_messages_rx.try_recv() {
                let peer_message = message_context.message;
                let peer_id = message_context.peer_id;
                let frost_identifier = match message_context.frost_identifier {
                    Some(frost_identifier) => frost_identifier,
                    None => {
                        error!(target: "consensus::authority::frost_task::start_task", "Frost identifier not found for peer id {:?}", peer_id);
                        continue;
                    }
                };

                match peer_message {
                    PeerMessageResponse::Error(err) => {
                        error!(target: "consensus::authority::frost_task::start_task", "Received error from peer {:?}: {:?}", peer_id, err);
                        continue;
                    }
                    PeerMessageResponse::WalletState(response) => {
                        // Only handle response if it has no state: responses
                        // with state are also
                        // sent to WalletStateSyncEngine::sync_wallet_state
                        // which updates the wallet state. This code block
                        // handles sending our wallet state to a peer
                        //
                        if Self::has_wallet_state(&response) {
                            info!(target: "consensus::authority::wallet_syncer::start_task", "Received wallet state in frost task from peer {:?}", peer_id);
                            continue;
                        }

                        // get all frost peers connections
                        let all_peers_handle = {
                            let (tx, rx) = tokio::sync::oneshot::channel();

                            let cmd = FrostCommand::GetAllConnectedPeers(tx);
                            if let Err(e) = self.frost_handle.send_command(cmd) {
                                error!(target: "consensus::authority::frost_task::start_task", "Error getting all peers handle {:?}", e);
                                continue;
                            }

                            rx.await.expect("expect all peers handle to exist")
                        };

                        info!(target: "consensus::authority::frost_task::start_task", "Got all peers handle");
                        if !all_peers_handle.contains_key(&peer_id) {
                            error!(target: "consensus::authority::frost_task::start_task", "Peer handle not found for peer id {:?}", peer_id);
                            continue;
                        }
                        let peer_handle =
                            all_peers_handle.get(&peer_id).expect("peer handle to exist");

                        if let Err(e) = self
                            .send_serialized_compressed_finalized_pegout_ids(
                                self.frost_config.wallet_state_sync_chunk_size,
                                peer_handle,
                                &response,
                            )
                            .await
                        {
                            error!(target: "consensus::authority::frost_task::start_task", "Error getting serialized compressed finalized pegout ids: {:?}", e);
                            continue;
                        }
                    }
                    PeerMessageResponse::Dkg(dkg_response) => {
                        let Some(tasks) = self.dkg_tasks.as_ref() else {
                            warn!(target: "consensus::authority::frost_task::start_task", "Dkg task is not running, dropping request...");
                            continue;
                        };
                        if let Some(task) = tasks.get(&dkg_response.multisig_id.into()) {
                            if let Err(err) = task.send(dkg_response).await {
                                warn!(target: "consensus::authority::frost_task::start_task", "Failed to send dkg response to task: {:?}", err);
                                continue;
                            }
                        }
                    }
                    PeerMessageResponse::Signing(signing_response) => {
                        let SigningResponse { response_type, signing_session_id, psbt } =
                            signing_response;
                        let signing_session_id = match FixedBytes::try_from(
                            signing_session_id.as_slice(),
                        ) {
                            Ok(signing_session_id) => signing_session_id,
                            Err(e) => {
                                error!(target: "consensus::authority::frost_task::start_task", "Error deserializing signing session id {:?}", e);
                                continue;
                            }
                        };
                        match response_type {
                            SigningEventResponseType::SignerRound1SigningPackage => {
                                let psbt_res = match bitcoin::Psbt::deserialize(psbt.as_slice()) {
                                    Ok(psbt) => psbt,
                                    Err(e) => {
                                        error!(target: "consensus::authority::frost_task::SignerRound1SigningPackage", "Error deserializing psbt {:?}", e);
                                        continue;
                                    }
                                };

                                if let Err(e) = validate_psbt_by_ids(
                                    &self.storage.reth_database,
                                    self.storage.btc_network,
                                    &psbt_res,
                                )
                                .await
                                {
                                    error!(target: "consensus::authority::frost_task::SignerRound1SigningPackage", "Error validating psbt {:?}", e);
                                    continue;
                                }

                                if let Err(e) = self
                                    .signing_state_machine
                                    .signer_process_round1(
                                        &frost_identifier,
                                        signing_session_id,
                                        psbt,
                                    )
                                    .await
                                {
                                    error!(target: "consensus::authority::frost_task::SignerRound1SigningPackage", "Peer Error processing round 1 signing {:?}", e);
                                }
                            }
                            SigningEventResponseType::CoordinatorRound1SigningPackage => {
                                let psbt_res = match bitcoin::Psbt::deserialize(psbt.as_slice()) {
                                    Ok(psbt) => psbt,
                                    Err(e) => {
                                        error!(target: "consensus::authority::frost_task::CoordinatorRound1SigningPackage", "Error deserializing psbt {:?}", e);
                                        continue;
                                    }
                                };

                                if let Err(e) = validate_psbt_by_ids(
                                    &self.storage.reth_database,
                                    self.storage.btc_network,
                                    &psbt_res,
                                )
                                .await
                                {
                                    error!(target: "consensus::authority::frost_task::CoordinatorRound1SigningPackage", "Error validating psbt {:?}", e);
                                    continue;
                                }

                                if let Err(e) = self
                                    .signing_state_machine
                                    .coordinator_process_round1(
                                        &frost_identifier,
                                        signing_session_id,
                                        psbt,
                                    )
                                    .await
                                {
                                    error!(target: "consensus::authority::frost_task::CoordinatorRound1SigningPackage", "Coordinator Error processing round 1 signing package {:?}", e);
                                }
                            }
                            SigningEventResponseType::SignerRound2SigningPackage => {
                                let psbt_res = match bitcoin::Psbt::deserialize(psbt.as_slice()) {
                                    Ok(psbt) => psbt,
                                    Err(e) => {
                                        error!(target: "consensus::authority::frost_task::SignerRound2SigningPackage", "Error deserializing psbt {:?}", e);
                                        continue;
                                    }
                                };

                                if let Err(e) = validate_psbt_by_ids(
                                    &self.storage.reth_database,
                                    self.storage.btc_network,
                                    &psbt_res,
                                )
                                .await
                                {
                                    error!(target: "consensus::authority::frost_task::SignerRound2SigningPackage", "Error validating psbt {:?}", e);
                                    continue;
                                }

                                if let Err(e) = self
                                    .signing_state_machine
                                    .signer_process_round2(
                                        &frost_identifier,
                                        signing_session_id,
                                        psbt,
                                    )
                                    .await
                                {
                                    error!(target: "consensus::authority::frost_task::SignerRound2SigningPackage", "Peer Error processing round 2 signing package {:?}", e);
                                }
                            }
                            SigningEventResponseType::CoordinatorRound2SigningPackage => {
                                let psbt_res = match bitcoin::Psbt::deserialize(psbt.as_slice()) {
                                    Ok(psbt) => psbt,
                                    Err(e) => {
                                        error!(target: "consensus::authority::frost_task::CoordinatorRound1SigningPackage", "Error deserializing psbt {:?}", e);
                                        continue;
                                    }
                                };

                                if let Err(e) = validate_psbt_by_ids(
                                    &self.storage.reth_database,
                                    self.storage.btc_network,
                                    &psbt_res,
                                )
                                .await
                                {
                                    error!(target: "consensus::authority::frost_task::CoordinatorRound1SigningPackage", "Error validating psbt {:?}", e);
                                    continue;
                                }

                                if let Err(e) = self
                                    .signing_state_machine
                                    .coordinator_process_round2(
                                        &frost_identifier,
                                        signing_session_id,
                                        psbt,
                                    )
                                    .await
                                {
                                    error!(target: "consensus::authority::frost_task::CoordinatorRound2SigningPackage", "Coordinator Error processing round 2 signing package {:?}", e);
                                }
                            }
                        }
                    }
                }
            }

            // short sleep
            tokio::time::sleep(std::time::Duration::from_millis(1250)).await;
        }
    }
}

impl<RDB, BDB, ToFrostMan, Source, BtcServerClient> std::fmt::Debug
    for FrostTask<RDB, BDB, ToFrostMan, Source, BtcServerClient>
where
    ToFrostMan: ToFrostManager + Clone,
    Source: RandomSource,
    BtcServerClient: BtcServerExtendedApi + Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrostTask").finish_non_exhaustive()
    }
}

struct DkgRunnerTask<RDB, BDB, ToFrostMan, BtcServerClient> {
    rx: mpsc::Receiver<DkgResponse>,
    // Frost network Handler
    frost_handle: ToFrostMan,
    // Frost Id lookup table
    frost_ids: HashMap<frost_secp256k1_tr::Identifier, secp256k1::PublicKey>,
    // Shared storage to insert aggregate public key
    storage: Storage<RDB, BDB>,
    // btc-server client
    btc_server: BtcServerClient,
    // Authority Metrics
    metrics: Arc<AuthorityMetrics>,
    // Multisig ID for this DKG task
    multisig_id: MultisigId,
}

impl<RDB, BDB, ToFrostMan, BtcServerClient>
    DkgRunnerTask<RDB, BDB, ToFrostMan, BtcServerClient>
where
    RDB: BlockReaderIdExt
        + StateProviderFactory
        + CanonStateSubscriptions
        + Clone
        + 'static
        + Send
        + Sync,
    BDB: Clone + 'static + Send + Sync,
    ToFrostMan: 'static + Send + Sync + ToFrostManager,
    BtcServerClient: BtcServerExtendedApi,
{
    #[allow(clippy::new_ret_no_self)]
    fn new(
        frost_handle: ToFrostMan,
        authorities: &[secp256k1::PublicKey],
        storage: Storage<RDB, BDB>,
        btc_server: BtcServerClient,
        metrics: Arc<AuthorityMetrics>,
        multisig_id: MultisigId,
    ) -> mpsc::Sender<DkgResponse> {
        let (tx, rx) = mpsc::channel(100);

        let frost_ids = authorities
            .iter()
            .enumerate()
            .map(|(index, pk)| {
                let frost_id =
                    authority_index_to_frost_identifier(index as u16);
                (frost_id, *pk)
            })
            .collect();

        let this = DkgRunnerTask {
            rx,
            frost_handle,
            frost_ids,
            storage,
            btc_server,
            metrics,
            multisig_id,
        };

        // Spawn-off the task, which will keep interacting with the btc-server.
        tokio::spawn(this.run());

        tx
    }
    async fn run(mut self) {
        const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

        // On startup, we call the btc-server immediately to get the initial
        // payloads. Only the coordinator will have something to send at this
        // point, while non-coordinators wait for the coordinators first message
        // before any messages get sent.
        let mut timeout = Duration::from_millis(0);

        loop {
            match tokio::time::timeout(timeout, self.rx.recv()).await {
                // Received a DKG payload from the frost task, forwarding to
                // btc-server.
                Ok(Some(dkg)) => {
                    let req = botanix_btc_server_client::DkgPayload {
                        sender: dkg.sender,
                        recipient: dkg.recipient,
                        payload: dkg.data,
                        multisig_id: *self.multisig_id,
                    };

                    let resp = match self.btc_server.new_dkg_payload(req).await
                    {
                        Ok(r) => r,
                        Err(err) => {
                            timeout = DEFAULT_TIMEOUT;
                            error!(target: "consensus::authority::frost_task::DkgRunnerTask", "Error sending dkg payload to btc server {:?}", err);
                            continue;
                        }
                    };

                    if let Ok(resp) = self
                        .btc_server
                        .get_public_key(
                            botanix_btc_server_client::GetPublicKeyRequest {
                                multisig_id: *self.multisig_id,
                            },
                        )
                        .await
                    {
                        self.metrics.created_agg_pub_keys.increment(1);

                        // decode the public key and assign it to the self
                        // variable
                        let public_key_package =
                            secp256k1::PublicKey::from_str(&resp.publickey)
                                .expect("invalid aggregated public key");

                        let mut storage = self.storage.write().await;
                        if let Some(agg_pks) =
                            storage.aggregate_public_key.as_mut()
                        {
                            agg_pks
                                .insert(self.multisig_id, public_key_package);
                        } else {
                            storage.aggregate_public_key =
                                Some(BTreeMap::from([(
                                    self.multisig_id,
                                    public_key_package,
                                )]));
                        }
                    }

                    // Update timeout at which point the btc-server should be
                    // called again.
                    timeout = Duration::from_millis(resp.timeout);

                    // Gossip the payloads to all frost peers.
                    if self.gossip_payloads(resp.payloads).await.is_err() {
                        error!(target: "consensus::authority::frost_task::DkgRunnerTask", "Failed to gossip payloads. Wait for the next message");
                        continue;
                    }
                }
                // Frost task dropped the handle, exiting...
                Ok(None) => {
                    info!(target: "consensus::authority::frost_task::DkgRunnerTask", "Received shutdown signal");
                    break;
                }
                // Timeout triggered, calling the btc-server to generate new
                // payloads.
                Err(_) => {
                    warn!(target: "consensus::authority::frost_task::DkgRunnerTask", "DKG timeout triggered");

                    let resp = match self
                        .btc_server
                        .get_dkg_payloads(
                            botanix_btc_server_client::GetDkgPayloadsRequest {
                                multisig_id: *self.multisig_id,
                            },
                        )
                        .await
                    {
                        Ok(r) => r,
                        Err(err) => {
                            timeout = DEFAULT_TIMEOUT;
                            error!(target: "consensus::authority::frost_task::DkgRunnerTask", "Error getting dkg payloads from btc server {:?}", err);
                            continue;
                        }
                    };

                    // Update timeout at which point the btc-server should be
                    // called again.
                    timeout = Duration::from_millis(resp.timeout);

                    if self.gossip_payloads(resp.payloads).await.is_err() {
                        error!(target: "consensus::authority::frost_task::DkgRunnerTask", "Failed to gossip payloads. Wait for the next message");
                        continue;
                    }
                }
            }
        }
    }

    async fn gossip_payloads(
        &self,
        payloads: Vec<botanix_btc_server_client::DkgPayload>,
    ) -> Result<(), FrostTaskError> {
        if payloads.is_empty() {
            return Ok(());
        }

        info!(target: "consensus::authority::frost_task::DkgRunnerTask", "Ready to gossip {} generated DKG payload(s)", payloads.len());

        // get all frost peers connections
        let all_peers_handles = {
            let (tx, rx) = tokio::sync::oneshot::channel();

            let cmd = FrostCommand::GetAllConnectedPeers(tx);
            if let Err(e) = self.frost_handle.send_command(cmd) {
                error!(target: "consensus::authority::frost_task::DkgRunnerTask", "Failed to send GetAllConnectedPeers frost command {}", e);
                return Err(FrostTaskError::UnableToGetAllConnectedPeers(e));
            }

            rx.await.expect("expect all peers handle to exist")
        };

        for payload in payloads {
            let recipient = frost_id_from_bytes(&payload.recipient)
                .expect("valid frost id");

            // Lookup the public key of the recipient.
            let Some(pk) = self.frost_ids.get(&recipient) else {
                error!(target: "consensus::authority::frost_task::DkgRunnerTask", "No Frost Id lookup available for recipient {:?}, dropping DKG payload...", recipient);
                continue;
            };

            let pk_string = pk.to_string();

            // TODO (lamafab): This could be improved, by using a hashmap or so.
            let Some(peer_data) = all_peers_handles
                .iter()
                .find(|(_, peer_data)| peer_data.frost_identifier == recipient)
                .map(|(_, peer_data)| peer_data)
            else {
                warn!(target: "consensus::authority::frost_task::DkgRunnerTask", "Peer handle not found for recipient {}, dropping DKG payload...", pk_string);
                continue;
            };

            let resp = PeerMessageResponse::Dkg(DkgResponse {
                data: payload.payload,
                sender: payload.sender,
                recipient: payload.recipient,
                multisig_id: payload.multisig_id,
            });

            match peer_data
                .peer_commands_tx
                .send(FrostPeerCommand::PeerMessage(resp))
            {
                Ok(_) => {
                    info!(target: "consensus::authority::frost_task::DkgRunnerTask", "Gossiping DKG payload to peer {}", pk_string);
                }
                Err(err) => {
                    error!(target: "consensus::authority::frost_task::DkgRunnerTask", "Error sending DKG payload to recipient {}: {:?}", pk_string, err);
                }
            }
        }

        Ok(())
    }
}
