use crate::{
    consensus::{
        signing::SigningStateMachine,
        utils::{
            get_dynafed_sub_msg_from_notification,
            get_pending_pegouts_from_pegout_data,
            get_pending_pegouts_from_staged_pegouts, get_sweep_psbt,
            get_utxos_from_pegin_meta, get_utxos_from_staged_pegins,
            is_poa_epoch, retry_exec, validate_psbt_by_ids,
        },
        Storage,
    },
    node::network::BotanixNetworkPrimitives,
    services::frost::MultisigConfig,
};
use alloy_consensus::{BlockHeader, Sealable};
use alloy_primitives::{Bytes, B256};
use bitcoin::consensus::Encodable;
use botanix_authority_edh::header_ext::HeaderExt;
use botanix_authority_metrics::AuthorityMetrics;
use botanix_authority_peg::peg_contract::{PeginMeta, PegoutWithId};
use botanix_authority_rsp::RandomSource;
use botanix_btc_server_client::{
    BtcServerExtendedApi, ConsensusCheckpointRequest, GrpcClientError,
    PendingPegout, SubscribeToDynafedNotificationsStream, Utxo,
};
use botanix_comet_bft_rpc::{
    Client, CometBftRpcFactory, HttpCometBFTRpcClientFactory,
};
use botanix_data_parser::{
    prost_parser::{ProstError, ProstMessageSerdelizer},
    DataParser, Error as DataParserError,
};
use botanix_storage::{StagedHeaderReader, StagedHeaderWriter};
use botanix_types::{MultisigId, LEGACY_MULTISIG_ID};
use btcserverlib::{
    dkg::{DkgNotification, DynafedSubscriptionMessage, SweepNotification},
    wallet::psbt::frost_id_from_bytes,
};
use eyre::{eyre, Context};
use frost_secp256k1_tr as frost;
use futures::{pin_mut, StreamExt};
use reth_network::{
    frost::{
        manager::{
            authority_index_to_frost_identifier, FrostCommand, PeerData,
            PeerMessageContext, ToFrostManager,
        },
        DkgResponse, FrostPeerCommand, PeerMessageResponse,
        SigningEventResponseType, SigningResponse, WalletStateResponse,
    },
    NetworkHandle,
};
use reth_primitives::NodePrimitives;
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

struct MultisigEntry<ToFrostMan, Source, BtcServerClient> {
    config: MultisigConfig,
    signing_sm: SigningStateMachine<ToFrostMan, Source, BtcServerClient>,
    /// A handle to the `DkgRunnerTask` task. This is only `Some` if no
    /// aggregate public key is available, and the `start_task` method has hence
    /// started the DKG process.
    dkg_task: Option<mpsc::Sender<DkgResponse>>,
}

#[allow(dead_code)]
pub struct FrostTask<RDB, BDB, ToFrostMan, Source, BtcServerClient> {
    /// Network Handler
    pub(crate) network_handle: NetworkHandle<BotanixNetworkPrimitives>,
    /// Frost network Handler
    pub(crate) frost_handle: ToFrostMan,
    /// Shared storage to insert aggregate public key
    pub(crate) storage: Storage<RDB, BDB>,
    //
    multisigs: BTreeMap<
        MultisigId,
        MultisigEntry<ToFrostMan, Source, BtcServerClient>,
    >,
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
    cbft_is_syncing: bool,
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
    BDB: StagedHeaderReader + StagedHeaderWriter + Clone + 'static,
    Source: RandomSource + Clone,
    BtcServerClient: BtcServerExtendedApi + Clone,
{
    /// Creates a new instance of the task
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        btc_server: BtcServerClient,
        network_handle: NetworkHandle<BotanixNetworkPrimitives>,
        frost_handle: ToFrostMan,
        multisig_configs: Vec<MultisigConfig>,
        storage: Storage<RDB, BDB>,
        compressor: DataParser,
        random_source_provider: Source,
        metrics: Arc<AuthorityMetrics>,
        cometbft_rpc_factory: HttpCometBFTRpcClientFactory,
        dynafed_frost_notifications_tx: tokio::sync::broadcast::Sender<SubscribeToDynafedNotificationsStream>,
    ) -> Self {
        // TODO: Do basic validation on multisig_configs?
        let multisigs = multisig_configs
            .into_iter()
            .map(|config| {
                // TODO: Update this log => it's weird.
                info!(
                    target: "consensus::authority::frost_task::new",
                    "Frost authority index: {}/{}",
                    config.authority_index, config.authorities.len() - 1
                );

                // Setup the signing state machine.
                let signing_sm = SigningStateMachine::new(
                    btc_server.clone(),
                    frost_handle.clone(),
                    config.clone(),
                    random_source_provider.clone(),
                    metrics.clone(),
                );

                let multisig_id = config.multisig_id;

                let entry = MultisigEntry {
                    config,
                    signing_sm,
                    // Might be set during [`FrostTask::start_task`].
                    dkg_task: None,
                };

                (multisig_id, entry)
            })
            .collect();

        // TODO: Return error instead?
        let cbft_rpc_provider =
            cometbft_rpc_factory.build_and_connect().expect("light client to connect");

        Self {
            network_handle,
            frost_handle,
            multisigs,
            storage,
            btc_server,
            check_staged_headers: true,
            cbft_is_syncing: true,
            compressor,
            metrics,
            cbft_rpc_provider,
            dynafed_frost_notifications_tx,
        }
    }
    // TODOs (lamafab):
    // * If this returns an error, then the entire Reth node should exit(?)
    // * Should this bother with ABCI sync checks?
    // * We get a couple of CometBFT service errors on startup, which eventually
    //   resolve. Probably related to the ABCIClient/Server startup mechanism.
    //   Look into it.
    pub async fn start_task(&mut self) -> eyre::Result<()>{
        // Request peer message channel from the Frost manager.
        let (callback_tx, callback_rx) = tokio::sync::oneshot::channel();

        self
            .frost_handle
            .send_command(FrostCommand::GetPeerMessagesStream(callback_tx))
            .wrap_err("Failed to send command to Frost manager")?;

        let mut peer_message_stream = callback_rx
            .await
            .wrap_err("Failed to received peer message handle from Frost manager")?;

        info!(
            target: "consensus::authority::frost_task::start_task",
            "Successfully acquired peer message stream",
        );

        // Prepare all multisigs entries, either be retrieving the aggregated
        // public keys from the BTC server or by starting the corresponding DKG
        // runner task.
        self.check_multisig_state()
            .await
            .wrap_err("Failed to complete multisig state check")?;

        let mut canon_state_stream = self.storage.reth_database.subscribe_to_canonical_state();
        let mut dynafed_stream = self.dynafed_frost_notifications_tx.subscribe();

        info!(
            target: "consensus::authority::frost_task::start_task",
            "Ready to enter primary event loop...",
        );

        debug_assert!(self.cbft_is_syncing);

        loop {
            // Check any staged headers; note that we return an error if this
            // fails, since we do not want to process any notifications after
            // such an occurrence.
            //
            // This method returns quickly if there are no staged headers.
            self.check_staged_headers()
                .await
                .wrap_err("Failed to handle staged headers")?;

            // Poll the streams for notifications as they arrive. The ABCI sync
            // status check is triggered periodically.
            //
            // TODO: We should have a re-subscribe mechanism for any stream
            // breakage => currently we just silently ignore such cases.
            let event_res = tokio::select! {
                msg = peer_message_stream.recv() => match msg {
                    Some(m) => self.on_peer_msg(m).await,
                    None => Err(eyre!("Peer message stream closed")),
                },
                msg = canon_state_stream.recv() => match msg {
                    Ok(m) => self.on_canon_state(m).await,
                    Err(e) => Err(eyre!("Canon state stream failed: {}", e)),
                },
                msg = dynafed_stream.recv() => match msg {
                    Ok(m) => self.on_dynafed(m).await,
                    Err(e) => Err(eyre!("Dynafed stream failed: {}", e)),
                },
            };

            if let Err(report) = event_res {
                error!(
                    target: "consensus::authority::frost_task::start_task",
                    "Failed to process event message {}", report
                );
            }
        }
    }
    // Checks the local database for any staged headers. Staged headers indicate
    // that pegins and pegouts were correctly extracted during finalization, but
    // the corresponding checkpoint has not yet been created on the btc-server.
    //
    // Note that this can happen if the connection to the btc-server has been
    // interrupted while block production is continuing.
    async fn check_staged_headers(&mut self) -> eyre::Result<()> {
        if !self.check_staged_headers {
            return Ok(());
        }

        let mut staged_headers = self
            .storage
            .botanix_database_factory
            .get_staged_headers()
            .expect("to get staged headers");

        if staged_headers.is_empty() {
            info!(
                target: "consensus::authority::frost_task::start_task",
                "No staged headers found, proceeding with frost task"
            );

            // NOTE: We let this method complete in full.
        } else {
            warn!(
                target: "consensus::authority::frost_task::start_task",
                "Found {} staged headers, proceeding with btc-server checkpoint reconstruction",
                staged_headers.len()
            );
        }

        // NOTE: This flag might be overridden by the
        // `handle_canon_state_commit` method.
        self.check_staged_headers = false;

        // Sort staged headers by block number in ascending order.
        staged_headers.sort_by(|(_, a), (_, b)| a.header.number.cmp(&b.header.number));

        for (header_hash, entry) in staged_headers {
            let header = entry.header;

            let pegins =
                get_utxos_from_staged_pegins(entry.pegins);
            let pegouts =
                get_pending_pegouts_from_staged_pegouts(entry.pegouts, header.timestamp);

            self.handle_canon_state_commit(header_hash, &header, pegins, pegouts).await?;
        }

        Ok(())
    }
    // Checks the aggregated public key for each configured federation multisig.
    // If no public key exists, the DKG process must be completed first, so a
    // [`DkgRunnerTask`] is started and its handler registered. The coordinator
    // is responsible for kicking off the DKG process.
    //
    // Note that DKG runner tasks can also be aborted and (re-)started via the
    // [`DkgNotification`] event triggered by the Botanix BTC server.
    async fn check_multisig_state(&mut self) -> eyre::Result<()> {
        if self.multisigs.is_empty() {
            return Err(eyre!("Empty multisig configuration not permitted"));
        }

        for (multisig_id, entry) in &mut self.multisigs {
            // TODO: The endpoint should return `Option<_>`, such that we can
            // actually properly distinguish between errors and missing
            // packages.
            let Ok(public_key) = self.btc_server.get_public_key(
                botanix_btc_server_client::GetPublicKeyRequest {
                    multisig_id: multisig_id.as_u32(),
                }
            ).await else {
                warn!(
                    target: "consensus::authority::frost_task::start_task",
                    "No public key found, proceeding with DKG"
                );

                // Start the dkg state machine task runner.
                let handle = DkgRunnerTask::start(
                    self.frost_handle.clone(),
                    &entry.config.authorities,
                    self.storage.clone(),
                    self.btc_server.clone(),
                    Arc::clone(&self.metrics),
                    *multisig_id,
                );

                entry.dkg_task = Some(handle);

                info!(
                    target: "consensus::authority::frost_task::start_task",
                    "DKG runner task started..."
                );

                continue;
            };

            info!(
                target: "consensus::authority::frost_task::start_task",
                "received aggregate public key from dkg state machine {:?}",
                public_key
            );

            let public_key = hex::decode(public_key.publickey)
                .wrap_err("Failed to hex-decode public key")?;

            let public_key = secp256k1::PublicKey::from_slice(&public_key)
                .wrap_err("Failed to create secp256k1 public key from slice")?;

            // Fill the Storage entry with the multisigs' public key.
            let mut storage = self.storage.inner.write().await;
            storage
                .aggregate_public_key
                .get_or_insert_default()
                .insert(*multisig_id, public_key);
        }

        Ok(())
    }
    /// Handles a dynafed notification from the btc-server. Dispatches to DKG
    /// start/abort or sweep handlers based on the message type.
    async fn on_dynafed(&mut self, notification: SubscribeToDynafedNotificationsStream) -> eyre::Result<()> {
        info!(
            target: "consensus::authority::frost_task::start_task",
            "Received dynafed frost notification from btc-server: {:?}",
            notification
        );

        let notification = notification
            .notification
            .ok_or(eyre!("Received dynafed frost notification with no payload"))?;

        let dynafed_sub_message = get_dynafed_sub_msg_from_notification(notification)
            .wrap_err("Failed to extract sub-message from Dynafed notification")?;

        match dynafed_sub_message {
            DynafedSubscriptionMessage::Dkg(msg) => {
                let multisig_id = msg.multisig_id();

                info!(
                    target: "consensus::authority::frost_task::start_task",
                    "Handling DKG notification for multisig id {}", multisig_id
                );

                match msg {
                    DkgNotification::Start { multisig_id } => {
                        self.on_dynafed_dkg_start(multisig_id)?;
                    }
                    DkgNotification::Abort { multisig_id } => {
                        self.on_dynafed_dkg_abort(multisig_id)?;
                    }
                }
            }
            DynafedSubscriptionMessage::Sweep(notification) => {
                self.on_dynafed_sweep(notification).await?;
            }
        }

        Ok(())
    }
    /// Routes an incoming peer message to the appropriate handler based on its
    /// type (DKG, signing, wallet state, or error).
    async fn on_peer_msg(&mut self, message_context: PeerMessageContext) -> eyre::Result<()> {
        let peer_message = message_context.message;
        let peer_id = message_context.peer_id;
        let frost_identifier = message_context.frost_identifier
            .ok_or(eyre!("Frost identifier not found for peer {}", peer_id))?;

        match peer_message {
            PeerMessageResponse::Dkg(dkg_response) => {
                self.on_peer_msg_dkg(dkg_response).await
            }
            PeerMessageResponse::Signing(signing_response) => {
                self.on_peer_msg_signing(frost_identifier, signing_response).await
            }
            PeerMessageResponse::WalletState(response) => {
                self.on_peer_msg_wallet_state(peer_id, response).await
            }
            PeerMessageResponse::Error(err) => {
                warn!(
                    target: "consensus::authority::frost_task::start_task",
                    "Received explicit error message from peer {peer_id}: {err}"
                );

                Ok(())
            }
        }
    }
    /// Starts a [`DkgRunnerTask`] for the given multisig. No-ops if one is
    /// already running.
    fn on_dynafed_dkg_start(&mut self, multisig_id: MultisigId) -> eyre::Result<()> {
        info!(
            target: "consensus::authority::frost_task::start_task",
            "Starting DKG for multisig id {}", multisig_id
        );

        let multisig = self
            .multisigs
            .get_mut(&multisig_id)
            .ok_or(eyre::eyre!("No multisig entry found for Id {}", multisig_id))?;

        if multisig.dkg_task.is_some() {
            warn!(
                target: "consensus::authority::frost_task::start_task",
                "DKG task runner for {multisig_id} already started, skipping...",
            );

            return Ok(())
        }

        // Start the dkg state machine task runner for that multisig id
        let handle = DkgRunnerTask::start(
            self.frost_handle.clone(),
            &multisig.config.authorities,
            self.storage.clone(),
            self.btc_server.clone(),
            Arc::clone(&self.metrics),
            multisig_id,
        );

        multisig.dkg_task = Some(handle);
        Ok(())
    }
    /// Aborts the running [`DkgRunnerTask`] for the given multisig by dropping
    /// its sender handle.
    fn on_dynafed_dkg_abort(&mut self, multisig_id: MultisigId) -> eyre::Result<()> {
        info!(
            target: "consensus::authority::frost_task::start_task",
            "Aborting DKG for multisig id {}", multisig_id
        );

        let multisig = self
            .multisigs
            .get_mut(&multisig_id)
            .ok_or(eyre::eyre!("No multisig entry found for Id {}", multisig_id))?;

        if multisig.dkg_task.take().is_none() {
            warn!(
                target: "consensus::authority::frost_task::start_task",
                "No DKG task runner for {multisig_id} is running, skipping...",
            );
        }

        debug_assert!(multisig.dkg_task.is_none());
        Ok(())
    }
    /// Handles a sweep notification by fetching and validating the sweep PSBT
    /// from the btc-server, then initiating a signing session (coordinator
    /// only).
    async fn on_dynafed_sweep(&mut self, notification: SweepNotification) -> eyre::Result<()> {
        info!(
            target: "consensus::authority::frost_task::start_task",
            "Handling Sweep notification: {} -> {}",
            notification.multisig_id_from, notification.multisig_id_to
        );

        // TODO: verify with migration state machine that the multisig ids
        // are valid and in the expected state for a sweep

        // Generate signing session ID
        let signing_session_id = FixedBytes::<32>::random();

        // Get sweep PSBT from btc-server
        let sweep_psbt_payload = get_sweep_psbt(
            &mut self.btc_server,
            &signing_session_id,
            notification.multisig_id_from,
            notification.multisig_id_to,
        )
        .await?;

        // Deserialize and validate PSBT is well-formed
        let sweep_psbt = bitcoin::Psbt::deserialize(&sweep_psbt_payload.psbt)
            .wrap_err("Failed to deserialize sweep PSBT")?;

        // Basic sanity checks
        if sweep_psbt.inputs.is_empty() {
            return Err(eyre!("Sweep PSBT has no inputs"));
        }

        if sweep_psbt.unsigned_tx.output.len() != 1 {
            return Err(eyre!(
                "Sweep PSBT should have exactly one output, got {}",
                sweep_psbt.unsigned_tx.output.len()
            ));
        }

        // TODO: other validations. Max number of inputs, output address,
        // fee rate, amounts

        let multisig = self
            .multisigs
            .get_mut(&notification.multisig_id_from)
            .ok_or(eyre::eyre!("No multisig entry found for Id {}", notification.multisig_id_from))?;

        // TODO: Verify whether it's okay to do this check here; originally it
        // was near the beginning.
        if !multisig.signing_sm.is_coordinator() {
            info!(
                target: "consensus::authority::frost_task::start_task",
                "Received sweep notification but we're not the coordinator"
            );

            return Ok(());
        }

        // Initiate signing session
        /*
        multisig
            .signing_sm
            .initiate_signing_session(signing_session_id, sweep_psbt_payload.psbt)
            .await
            .wrap_err("Failed to initiate signing session")?;

        info!(
            target: "consensus::authority::frost_task::start_task",
            "Started sweep signing session successfully"
        );
        */

        Err(eyre!("Currently unsupported (WIP)"))
    }
    /// Handles a canonical state notification. Only processes `Commit`
    /// variants; other notification types are ignored.
    async fn on_canon_state(&mut self, notification: CanonStateNotification<<RDB as NodePrimitivesProvider>::Primitives>
    ) -> eyre::Result<()> {
        info!(
            target: "consensus::authority::frost_task::start_task",
            "canon state notification received for block number {:?}",
            notification.tip().number()
        );

        match notification {
            CanonStateNotification::Commit { new, pegins, pegouts } => {
                self.on_canon_state_commit(new, pegins, pegouts).await?;
            }
            _ => {
                // Ignore other notifications
            }
        }

        Ok(())
    }
    /// Processes a canonical chain commit: deserializes pegins/pegouts from the
    /// new chain tip and delegates to [`Self::handle_canon_state_commit`].
    async fn on_canon_state_commit(
        &mut self,
        new: Arc<reth_execution_types::Chain<<RDB as NodePrimitivesProvider>::Primitives>>,
        pegins: Option<Vec<Bytes>>,
        pegouts: Option<Vec<Bytes>>,
    ) -> eyre::Result<()> {
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
            // Since we are skipping the current block, we need to check for staged headers on the next iteration.
            self.check_staged_headers = true;

            return Err(eyre!("No aggregate public keys found"));
        };

        // Convert pegins into the appropriate format
        let pegins = pegins.as_ref().map_or_else(Vec::new, |pegins| {
            let deserialized = pegins
                .iter()
                .filter_map(|bytes| match PeginMeta::deserialize(bytes) {
                    Ok((pegin, _)) => Some(pegin),
                    Err(e) => {
                        // TODO: Okay to just silently drop this?
                        error!("Failed to deserialize pegin: {:?}", e);
                        None
                    }
                }).collect::<Vec<_>>();

                get_utxos_from_pegin_meta(deserialized.as_slice(), &aggregate_public_keys)
            });

        // Convert pegouts into the appropriate format
        let pending_pegouts = pegouts.as_ref().map_or_else(Vec::new, |pegouts| {
            let deserialized = pegouts
                .iter()
                .filter_map(|bytes| match PegoutWithId::deserialize(bytes) {
                    Ok(pegout) => Some(pegout),
                    Err(e) => {
                        // TODO: Okay to just silently drop this?
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
        .await?;

        Ok(())
    }
    /// Handles the canon state commit notification by submitting the pegins and
    /// pegouts to the btc-server and initiating a checkpoint and signing
    /// session, if necessary.
    async fn handle_canon_state_commit<H>(
        &mut self,
        // Note: `header_hash` is the hash of the header that is being
        // committed. We pass this on such that calling `header.hash_slow()` can
        // be avoided. Ideally, this is already precomputed when passed on.
        header_hash: B256,
        header: &H,
        pegins: Vec<Utxo>,
        pending_pegouts: Vec<PendingPegout>,
    ) -> eyre::Result<()>
    where
        H: BlockHeader + Sealable + HeaderExt,
    {
        debug_assert_eq!(header_hash, header.hash_slow());

        info!(
            target: "consensus::authority::frost_task::handle_canon_state_commit",
            "Handling canon state commit for block number {:?}", header.number()
        );

        let edh = header.deserialize_extra_data_header().wrap_err("Failed to deserialize extra data header")?;

        let cp_block_hash = edh.bitcoin_block_hash;
        let mut block_hash_writer = vec![];
        cp_block_hash.consensus_encode(&mut block_hash_writer).wrap_err("Failed to encode checkpoint block hash")?;

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
                // Indicate to the Frost task that we should check staged
                // headers on the next iteration. Ideally, the connection to the
                // btc-server is restored at some point later and the staged
                // headers can be processed.
                self.check_staged_headers = true;

                return Err(eyre::Error::from(err)).wrap_err("Failed to send checkpoint to BTC server")
            }
        }

        // Check if this is an epoch block and if we are the coordinator. If
        // yes, initiate signing session.
        if !is_poa_epoch(header.number(), self.storage.chain_spec.epoch_length) {
            return Ok(());
        }

        // TODO: Support multi-federation setup.
        let multisig = self
            .multisigs
            .get_mut(&LEGACY_MULTISIG_ID)
            .ok_or(eyre::eyre!("No multisig entry found for Id {}", LEGACY_MULTISIG_ID))?;

        if !multisig.signing_sm.is_coordinator() {
            info!(
                target: "consensus::authority::frost_task::handle_canon_state_commit",
                "Received canon state notification during epoch block but we're not the coordinator"
            );

            return Ok(());
        }

        // Create psbt and send init signing message.
        let psbt_payload = crate::consensus::utils::get_psbt(
            &mut self.btc_server,
            &header_hash,
            cp_block_hash,
            LEGACY_MULTISIG_ID, // TODO: replace
            LEGACY_MULTISIG_ID, // TODO: replace
        )
        .await
        .wrap_err("Failed to retrieve PSBT from the BTC server")?;

        // Validate psbt.
        let psbt = bitcoin::Psbt::deserialize(psbt_payload.psbt.as_slice())
            .wrap_err("Failed to deserialize PSBT")?;

        validate_psbt_by_ids(&self.storage.reth_database, self.storage.btc_network, &psbt).await
            .wrap_err("Failed to validate PSBT by Ids")?;

        info!(
            target: "consensus::authority::frost_task::handle_canon_state_commit",
            "Validated psbt successfully"
        );

        // Initiate signing session.
        multisig
            .signing_sm
            .initiate_signing_session(header_hash, psbt_payload.psbt)
            .await
            .wrap_err("Error starting new signing session")?;

        info!(
            target: "consensus::authority::frost_task::handle_canon_state_commit",
            "Started new signing session successfully"
        );

        Ok(())
    }
    /// Handles wallet state requests from peers. If the response contains data
    /// it's a no-op (handled by `WalletStateSyncEngine`); otherwise sends our
    /// finalized pegout IDs back to the requesting peer.
    async fn on_peer_msg_wallet_state(&mut self, peer_id: FixedBytes<64>, response: WalletStateResponse) -> eyre::Result<()> {
        // Only handle response if it has no state: responses with state are
        // also sent to WalletStateSyncEngine::sync_wallet_state which updates
        // the wallet state. This code block handles sending our wallet state to
        // a peer
        if !response.finalized_pegout_ids.is_empty() {
            info!(
                target: "consensus::authority::wallet_syncer::start_task",
                "Received wallet state in frost task from peer {peer_id:?}"
            );

            return Ok(());
        }

        // get all frost peers connections
        let all_peers_handle = {
            let (tx, rx) = tokio::sync::oneshot::channel();

            let cmd = FrostCommand::GetAllConnectedPeers(tx);
            self.frost_handle.send_command(cmd).wrap_err("Error getting all peers handle")?;

            // TODO: Is this appropriate?
            rx.await.expect("expect all peers handle to exist")
        };

        info!(target: "consensus::authority::frost_task::start_task", "Got all peers handle");
        if !all_peers_handle.contains_key(&peer_id) {
            return Err(eyre!("Peer handle not found for peer id {:?}", peer_id));
        }

        let peer_handle =
            all_peers_handle.get(&peer_id).expect("peer handle to exist");

        // TODO: This is the default value => should be configurable.
        let chunk_size = 10;
        self
            .send_serialized_compressed_finalized_pegout_ids(
                //self.frost_config.wallet_state_sync_chunk_size,
                chunk_size,
                peer_handle,
                &response,
            )
            .await
            .wrap_err("Error getting serialized compressed finalized pegout ids")
    }
    /// Forwards a DKG response from a peer to the corresponding
    /// [`DkgRunnerTask`] via its mpsc channel.
    async fn on_peer_msg_dkg(&mut self, dkg_response: DkgResponse) -> eyre::Result<()> {
        let multisig = self
            .multisigs
            .get(&dkg_response.multisig_id.into())
            .ok_or(eyre::eyre!("No multisig entry found for Id {}", dkg_response.multisig_id))?;

        let Some(task) = multisig.dkg_task.as_ref() else {
            // TODO: log message
            warn!(target: "consensus::authority::frost_task::start_task", "TODO");
            return Ok(());
        };

        task.try_send(dkg_response).wrap_err("Failed to send DKG response to DKG runner task")
    }
    /// Handles a signing protocol message from a peer: validates the PSBT and
    /// dispatches to the appropriate signing state machine round
    /// (round1/round2, signer/coordinator).
    async fn on_peer_msg_signing(&mut self, frost_identifier: frost::Identifier, signing_response: SigningResponse) -> eyre::Result<()> {
        let SigningResponse { response_type, signing_session_id, psbt } =
            signing_response;

        let signing_session_id = FixedBytes::try_from(
            signing_session_id.as_slice(),
        ).wrap_err("Error deserializing signing session Id")?;

        let psbt_der = bitcoin::Psbt::deserialize(psbt.as_slice())
            .wrap_err("Failed to deserialize PSBT")?;

        validate_psbt_by_ids(
            &self.storage.reth_database,
            self.storage.btc_network,
            &psbt_der,
        )
        .await
        .wrap_err("Failed to validate PSBT by Ids")?;

        // TODO: Support multi-federation setup.
        let multisig = self
            .multisigs
            .get_mut(&LEGACY_MULTISIG_ID)
            .ok_or(eyre::eyre!("No multisig entry found for Id {}", LEGACY_MULTISIG_ID))?;

        // TODO: Maybe the signing state machine should take the deserialized
        // `Psbt` structure directly?
        match response_type {
            SigningEventResponseType::SignerRound1SigningPackage => {
                multisig
                    .signing_sm
                    .signer_process_round1(
                        &frost_identifier,
                        signing_session_id,
                        psbt,
                    )
                    .await
                    .wrap_err("SigningSM failed to process round1 package as signer")
            }
            SigningEventResponseType::CoordinatorRound1SigningPackage => {
                multisig
                    .signing_sm
                    .coordinator_process_round1(
                        &frost_identifier,
                        signing_session_id,
                        psbt,
                    )
                    .await
                    .wrap_err("SigningSM failed to process round1 package as coordinator")
            }
            SigningEventResponseType::SignerRound2SigningPackage => {
                multisig
                    .signing_sm
                    .signer_process_round2(
                        &frost_identifier,
                        signing_session_id,
                        psbt,
                    )
                    .await
                    .wrap_err("SigningSM failed to process round2 package as signer")
            }
            SigningEventResponseType::CoordinatorRound2SigningPackage => {
                multisig
                    .signing_sm
                    .coordinator_process_round2(
                        &frost_identifier,
                        signing_session_id,
                        psbt,
                    )
                    .await
                    .wrap_err("SigningSM failed to process round2 package as coordinator")
            }
        }
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
    fn start(
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
                            error!(
                                target: "consensus::authority::frost_task::DkgRunnerTask",
                                "Error sending dkg payload to btc server {err:?}",
                            );

                            timeout = DEFAULT_TIMEOUT;
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
                        error!(
                            target: "consensus::authority::frost_task::DkgRunnerTask",
                            "Failed to gossip payloads. Wait for the next message"
                        );

                        continue;
                    }
                }
                // Frost task dropped the handle, exiting...
                Ok(None) => {
                    info!(
                        target: "consensus::authority::frost_task::DkgRunnerTask",
                        "Received shutdown signal"
                    );

                    break;
                }
                // Timeout triggered, calling the btc-server to generate new
                // payloads.
                Err(_) => {
                    warn!(
                        target: "consensus::authority::frost_task::DkgRunnerTask",
                        "DKG timeout triggered"
                    );

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
                            error!(
                                target: "consensus::authority::frost_task::DkgRunnerTask",
                                "Error getting dkg payloads from btc server {err:?}"
                            );

                            continue;
                        }
                    };

                    // Update timeout at which point the btc-server should be
                    // called again.
                    timeout = Duration::from_millis(resp.timeout);

                    if self.gossip_payloads(resp.payloads).await.is_err() {
                        error!(
                            target: "consensus::authority::frost_task::DkgRunnerTask",
                            "Failed to gossip payloads. Wait for the next message"
                        );

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

        info!(
            target: "consensus::authority::frost_task::DkgRunnerTask",
            "Ready to gossip {} generated DKG payload(s)", payloads.len()
        );

        // get all frost peers connections
        let all_peers_handles = {
            let (tx, rx) = tokio::sync::oneshot::channel();

            let cmd = FrostCommand::GetAllConnectedPeers(tx);
            if let Err(e) = self.frost_handle.send_command(cmd) {
                error!(
                    target: "consensus::authority::frost_task::DkgRunnerTask",
                    "Failed to send GetAllConnectedPeers frost command {e}"
                );

                return Err(FrostTaskError::UnableToGetAllConnectedPeers(e));
            }

            rx.await.expect("expect all peers handle to exist")
        };

        for payload in payloads {
            let recipient = frost_id_from_bytes(&payload.recipient)
                .expect("valid frost id");

            // Lookup the public key of the recipient.
            let Some(pk) = self.frost_ids.get(&recipient) else {
                error!(
                    target: "consensus::authority::frost_task::DkgRunnerTask",
                    "No Frost Id lookup available for recipient {:?}, dropping DKG payload...",
                    recipient
                );

                continue;
            };

            let pk_string = pk.to_string();

            // TODO (lamafab): This could be improved, by using a hashmap or so.
            let Some(peer_data) = all_peers_handles
                .iter()
                .find(|(_, peer_data)| peer_data.frost_identifier == recipient)
                .map(|(_, peer_data)| peer_data)
            else {
                warn!(
                    target: "consensus::authority::frost_task::DkgRunnerTask",
                    "Peer handle not found for recipient {}, dropping DKG payload...",
                    pk_string
                );

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
                    info!(
                        target: "consensus::authority::frost_task::DkgRunnerTask",
                        "Gossiped DKG payload to peer {}", pk_string
                    );
                }
                Err(err) => {
                    error!(
                        target: "consensus::authority::frost_task::DkgRunnerTask",
                        "Error sending DKG payload to recipient {}: {:?}",
                        pk_string, err
                    );
                }
            }
        }

        Ok(())
    }
}
