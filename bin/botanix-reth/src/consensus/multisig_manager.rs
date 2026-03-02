use botanix_storage::{
    models::MultisigRecord, BotanixGuardedFactory, BotanixProviderFactory,
};
use botanix_types::{MultisigId, LEGACY_MULTISIG_ID};
use frost_secp256k1_tr::{self as frost, keys::PublicKeyPackage};
use merlin::Transcript;
use reth_db::Database;
use reth_network::frost::manager::authority_index_to_frost_identifier;
use reth_node_types::NodeTypes;
use reth_provider::{
    providers::NodeTypesForProvider, ProviderError, ProviderResult,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    str::FromStr,
    sync::{mpsc, Arc, Mutex},
};
use tokio::sync::oneshot;

// Re-exports
pub use botanix_storage::{
    models::{AttestedMultisigEntry, MultisigStatus, StagedMultisigEntry},
    MultisigManagerReader, MultisigManagerWriter,
};

/// Type alias for [`MultisigManager`] backed by the concrete Botanix database.
// TODO: Extend documentation and justify this.
pub type BotanixMultisigManager<DB, N> =
    MultisigManager<BotanixGuardedFactoryExt<DB, N>>;

impl<DB, N> BotanixMultisigManager<DB, N>
where
    DB: Database + Clone,
    N: NodeTypes + NodeTypesForProvider,
{
    /// Creates a new [`BotanixMultisigManager`] from a provider factory.
    ///
    /// This is a convenience constructor that wraps the factory in a
    /// [`BotanixGuardedFactory`] for transaction lifecycle management, then
    /// wraps that in [`BotanixGuardedFactoryExt`] to provide the
    /// [`MultisigManagerReader`] and [`MultisigManagerWriter`] implementations.
    ///
    /// Returns the manager and its associated [`MultisigSubmitter`] handle.
    pub fn new_botanix(
        factory: BotanixProviderFactory<DB, N>,
        legacy_preload: Option<AttestedMultisigEntry>,
    ) -> Result<(Self, MultisigSubmitter), Error> {
        let guard = BotanixGuardedFactory::new(factory);
        let ext = BotanixGuardedFactoryExt::new(guard);
        ext.db.start()?;

        let this: Result<_, _> = Self::new(ext.clone(), legacy_preload);
        if this.is_ok() {
            ext.db.commit()?;
        } else {
            ext.db.rollback()?;
        }

        this
    }
    /// Convenience method that executes a closure within a transaction that is
    /// rolled back afterward.
    pub fn guard_rollback<T>(
        &self,
        f: impl FnOnce(&Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.client.db.start()?;
        let result: Result<_, _> = f(self);

        // Always rollback, whether the closure failed or not.
        self.client.db.rollback()?;

        result
    }
    /// Convenience method that executes a closure within a transaction that is
    /// committed afterward.
    pub fn guard_commit<T>(
        &self,
        f: impl FnOnce(&Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.client.db.start()?;
        let result: Result<_, _> = f(self);

        // Commit on success, rollback on failure.
        if result.is_ok() {
            self.client.db.commit()?;
        } else {
            self.client.db.rollback()?;
        }

        result
    }
}

/// Wrapper extension around [`BotanixGuardedFactory`] that implements
/// [`MultisigManagerReaderWriter`]. Consider using
/// [`BotanixMultisigManager::new_botanix`] directly, instead.
#[derive(Debug, Clone)]
pub struct BotanixGuardedFactoryExt<DB, N>
where
    DB: Database,
    N: NodeTypes + NodeTypesForProvider,
{
    db: BotanixGuardedFactory<DB, N>,
}

impl<DB, N> BotanixGuardedFactoryExt<DB, N>
where
    DB: Database,
    N: NodeTypes + NodeTypesForProvider,
{
    /// Creates a new [`BotanixGuardedFactoryExt`] wrapper.
    pub fn new(db: BotanixGuardedFactory<DB, N>) -> Self {
        Self { db }
    }
}

impl<DB, N> MultisigManagerReader for BotanixGuardedFactoryExt<DB, N>
where
    DB: Database,
    N: NodeTypes + NodeTypesForProvider,
{
    fn get_staging_multisig(
        &self,
        id: MultisigId,
    ) -> ProviderResult<Option<StagedMultisigEntry>> {
        self.db.guard()?.get_staging_multisig(id)
    }
    fn get_attested_multisig(
        &self,
        id: MultisigId,
    ) -> ProviderResult<Option<AttestedMultisigEntry>> {
        self.db.guard()?.get_attested_multisig(id)
    }
    fn get_active_multisigs(
        &self,
    ) -> ProviderResult<Vec<AttestedMultisigEntry>> {
        self.db.guard()?.get_active_multisigs()
    }
    fn get_multisig_records(&self) -> ProviderResult<Vec<MultisigRecord>> {
        self.db.guard()?.get_multisig_records()
    }
}

impl<DB, N> MultisigManagerWriter for BotanixGuardedFactoryExt<DB, N>
where
    DB: Database,
    N: NodeTypes + NodeTypesForProvider,
{
    fn insert_staging_multisig(
        &self,
        id: MultisigId,
        entry: StagedMultisigEntry,
    ) -> ProviderResult<()> {
        self.db.guard()?.insert_staging_multisig(id, entry)
    }
    fn insert_attested_multisig(
        &self,
        id: MultisigId,
        entry: AttestedMultisigEntry,
    ) -> ProviderResult<()> {
        self.db.guard()?.insert_attested_multisig(id, entry)
    }
    fn remove_multisig(&self, id: MultisigId) -> ProviderResult<bool> {
        self.db.guard()?.remove_multisig(id)
    }
}

/// Errors from multisig lifecycle operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    /// Participant is not a federation member.
    #[error("not a member of the federation")]
    NotAFedMember,
    /// Signature verification failed.
    #[error("signature verification failed")]
    SignatureVerificationFailed,
    /// Missing signature shares for n-of-n aggregation.
    #[error("not enough signature shares available")]
    NotEnoughSignatureShares,
    /// Internal FROST library error.
    #[error("internal FROST error: {0}")]
    Frost(#[from] frost::Error),
    /// Operation requires staging lifecycle state.
    #[error("lifecycle must be staging")]
    LifecycleMustBeStaged,
    /// Operation requires accumulated lifecycle state.
    #[error("lifecycle must be accumulated")]
    LifecycleMustBeAccumulated,
    /// Operation requires funding lifecycle state.
    #[error("lifecycle must be funding")]
    LifecycleMustBeFunded,
    /// Operation requires degraded lifecycle state.
    #[error("lifecycle must be degrading")]
    LifecycleMustBeDegraded,
    /// Expiration requires sunset lifecycle state.
    #[error("lifecycle must be sunset for expiration")]
    LifecycleMustBeSunset,
    /// Multiple staging multisig are currently available, which is prohibited.
    #[error("multiple staging multisigs available (prohibited)")]
    MultipleStagingMultisigs,
    /// No funding multisig is currently available.
    #[error("no funding multisig available")]
    NoFundingMultisig,
    /// Multiple funding multisig are currently available, which is prohibited.
    #[error("multiple funding multisigs available (prohibited)")]
    MultipleFundingMultisigs,
    /// Manager has shut down.
    #[error("manager has shut down")]
    Shutdown,
    /// Database operation failed.
    #[error("database error: {0}")]
    Database(#[from] ProviderError),
}

struct ChannelPayload {
    message: Message,
    callback: oneshot::Sender<Result<(), Error>>,
}

/// Consensus message for multisig lifecycle state transitions.
///
/// These messages are proposed via [`MultisigManager::send`] and applied via
/// [`MultisigManager::recv`]. They are embedded in the non-deterministic data
/// (NDD) of block proposals to ensure all validators process the same state
/// transitions deterministically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    /// Activates a staging multisig after successful DKG completion.
    ///
    /// Contains the complete attestation proving all federation members
    /// participated correctly in the DKG ceremony. Upon validation, the
    /// multisig transitions from [`MultisigLifecycle::Staged`] to
    /// [`MultisigLifecycle::Active`], and any existing active multisigs are
    /// moved to [`MultisigLifecycle::Sunset`].
    Attestation {
        /// The multisig to activate.
        multisig_id: MultisigId,
        /// The FROST public key package from DKG.
        public_key_package: PublicKeyPackage,
        /// The signing package used for attestation signatures.
        signing_package: frost::SigningPackage,
        /// Per-member signature shares and attestation signatures.
        ///
        /// Each entry maps a FROST identifier to its signature share (for
        /// aggregation) and secp256k1 attestation signature (for identity
        /// binding).
        signatures: BTreeMap<
            frost::Identifier,
            (frost::round2::SignatureShare, secp256k1::ecdsa::Signature),
        >,
    },
    Sunsetting {
        /// The multisig to sunset.
        multisig_id: MultisigId,
        /// Coordinator's signature over the expiration commitment.
        coordinator_signature: secp256k1::ecdsa::Signature,
    },
    /// Removes a sunset multisig from the manager.
    ///
    /// Only the designated coordinator can authorize expiration by signing a
    /// Merlin transcript commitment binding the multisig ID and public key
    /// package. The multisig MUST be in the [`MultisigLifecycle::Sunset`]
    /// state.
    Expiration {
        /// The multisig to expire.
        multisig_id: MultisigId,
        /// Coordinator's signature over the expiration commitment.
        coordinator_signature: secp256k1::ecdsa::Signature,
    },
}

impl Message {
    /// Serialize this message to CBOR writer.
    pub fn serialize<W>(
        &self,
        writer: W,
    ) -> Result<(), ciborium::ser::Error<std::io::Error>>
    where
        W: std::io::Write,
    {
        ciborium::into_writer(self, writer)
    }

    /// Serialize this message to CBOR bytes.
    pub fn serialize_vec(&self) -> Vec<u8> {
        let mut bytes = vec![];
        self.serialize(&mut bytes).expect("writer must not fail");
        bytes
    }

    /// Deserialize a message from CBOR reader.
    pub fn deserialize<R>(
        reader: R,
    ) -> Result<Self, ciborium::de::Error<std::io::Error>>
    where
        R: std::io::Read,
    {
        ciborium::from_reader(reader)
    }

    /// Deserialize a message from CBOR bytes.
    pub fn deserialize_slice(
        bytes: &[u8],
    ) -> Result<Self, ciborium::de::Error<std::io::Error>> {
        Self::deserialize(bytes)
    }
}

/// Handle for awaiting the result of a submitted attestation or expiration.
///
/// Can be polled asynchronously via `.await`, blocked on via `blocking_recv()`,
/// or checked non-blocking via `try_recv()`. Returns `Error::Shutdown` if the
/// manager was shut down before processing the submission.
#[derive(Debug)]
pub struct SubmissionCallback {
    rx: oneshot::Receiver<Result<(), Error>>,
}

impl SubmissionCallback {
    /// Non-blocking attempt to receive the result.
    ///
    /// Returns `Ok(result)` if the callback has completed, `Err(self)` if the
    /// result is not yet available (allowing retry), or
    /// `Ok(Err(Error::Shutdown))` if the manager shut down.
    pub fn try_recv(mut self) -> Result<Result<(), Error>, Self> {
        match self.rx.try_recv() {
            Ok(result) => Ok(result),
            Err(oneshot::error::TryRecvError::Empty) => Err(self),
            Err(oneshot::error::TryRecvError::Closed) => {
                Ok(Err(Error::Shutdown))
            }
        }
    }
    /// Blocks the current thread until the result is available.
    ///
    /// Returns `Error::Shutdown` if the manager shut down before responding.
    pub fn blocking_recv(self) -> Result<(), Error> {
        self.rx.blocking_recv().map_err(|_| Error::Shutdown)?
    }
}

impl std::future::Future for SubmissionCallback {
    type Output = Result<(), Error>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::pin::Pin::new(&mut self.rx).poll(cx).map(|res| match res {
            Ok(Ok(r)) => Ok(r),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(Error::Shutdown),
        })
    }
}

/// Client handle for submitting messages to the multisig manager.
///
/// Supports submitting attestations (to activate staging multisigs) and
/// expirations (to remove sunset multisigs). Messages are validated before
/// being proposed to consensus.
// TODO: Update comment
// TODO: Update methods
#[derive(Debug, Clone)]
pub struct MultisigSubmitter {
    queue: mpsc::Sender<ChannelPayload>,
}

impl MultisigSubmitter {
    /// Submits a FROST attestation for validation and consensus.
    ///
    /// Returns a callback that resolves once the manager has processed the
    /// submission. If the manager has shut down, the callback resolves
    /// immediately with `Error::Shutdown`.
    pub fn submit_attestation(
        &self,
        multisig_id: MultisigId,
        public_key_package: PublicKeyPackage,
        signing_package: frost::SigningPackage,
        signatures: BTreeMap<
            frost::Identifier,
            (frost::round2::SignatureShare, secp256k1::ecdsa::Signature),
        >,
    ) -> SubmissionCallback {
        self.send_message(Message::Attestation {
            multisig_id,
            public_key_package,
            signing_package,
            signatures,
        })
    }
    /// Submits an expiration request for a sunset multisig.
    ///
    /// Returns a callback that resolves once the manager has processed the
    /// submission. If the manager has shut down, the callback resolves
    /// immediately with `Error::Shutdown`.
    pub fn submit_expiration(
        &self,
        multisig_id: MultisigId,
        coordinator_signature: secp256k1::ecdsa::Signature,
    ) -> SubmissionCallback {
        self.send_message(Message::Expiration {
            multisig_id,
            coordinator_signature,
        })
    }
    fn send_message(&self, message: Message) -> SubmissionCallback {
        let (callback, rx) = oneshot::channel();
        let payload = ChannelPayload { message, callback };

        // If the receiver has shut down, deposit an error into the callback
        // so it resolves immediately with `Error::Shutdown`.
        if let Err(err) = self.queue.send(payload) {
            err.0
                .callback
                .send(Err(Error::Shutdown))
                .expect("oneshot receiver exists");
        }

        SubmissionCallback { rx }
    }
}

/// Manages the lifecycle of FROST multisigs within the federation.
///
/// Tracks multisigs through their lifecycle states (Staged -> Active -> Sunset
/// -> implicitly expired) and processes attestations and expirations submitted
/// via [`MultisigSubmitter`].
///
/// # Architecture
///
/// The manager uses a channel-based design where [`MultisigSubmitter`] clients
/// submit messages that are validated and potentially forwarded to consensus.
/// Each submission returns a [`SubmissionCallback`] for tracking the result.
///
/// # Usage
///
/// ```ignore
/// let (mut manager, submitter) = MultisigManager::new();
///
/// // Register a new multisig
/// manager.set_staging(multisig_id, coordinator, fed_members);
///
/// // Client submits attestation via submitter
/// let callback = submitter.submit_attestation(...);
///
/// // Manager processes pending messages
/// if let Some(msg) = manager.send() {
///     // Propose message to consensus
/// }
/// ```
#[derive(Debug, Clone)]
pub struct MultisigManager<DB> {
    // TODO: Consider using a sync/bounded channel?
    queue: Arc<Mutex<mpsc::Receiver<ChannelPayload>>>,
    submitter: MultisigSubmitter,
    client: DB,
}

// TODO: Those `set_*` methods should only be available during building.
impl<DB> MultisigManager<DB>
where
    DB: MultisigManagerReader + MultisigManagerWriter,
{
    /// Creates a new manager and its associated [`MultisigSubmitter`] handle by
    /// loading and validating existing multisig state from the database.
    ///
    /// Scans all persisted multisig records to populate the in-memory cache of
    /// the current `staging` and `funding` multisig IDs. Fails if the database
    /// state is inconsistent (e.g. multiple staging or funding multisigs, or no
    /// funding multisig at all).
    // TODO: Update docs
    pub fn new(
        client: DB,
        legacy_preload: Option<AttestedMultisigEntry>,
    ) -> Result<(Self, MultisigSubmitter), Error> {
        let (tx, rx) = mpsc::channel();

        let mut staging: Option<MultisigId> = None;
        let mut funding: Option<MultisigId> = None;

        // Preload staging/funding IDs from persisted state. This also acts as
        // an integrity check: we reject startup if invariants are violated
        // (e.g. more than one staging or funding multisig).
        //
        // NOTE: Linear scan is acceptable here since the total number of
        // multisig records is tiny in practice (one or two).
        for record in client.get_multisig_records()? {
            match record {
                MultisigRecord::Staged { entry } => {
                    // At most one multisig may be staging at a time.
                    if staging.is_some() {
                        return Err(Error::MultipleStagingMultisigs);
                    }

                    staging = Some(entry.multisig_id);
                }
                MultisigRecord::Attested { entry } => {
                    if entry.status == MultisigStatus::Funding {
                        // At most one multisig may be in the Funding state.
                        if funding.is_some() {
                            return Err(Error::MultipleFundingMultisigs);
                        }

                        funding = Some(entry.multisig_id);
                    }
                }
            }
        }

        // Legacy migration: if no funding multisig was found in the database,
        // bootstrap one from the preloaded legacy entry. Safe to remove once
        // all nodes have migrated past the [`MultisigManager`] introduction.
        match (legacy_preload, funding) {
            (Some(legacy), None) => {
                let legacy_id = legacy.multisig_id;

                if legacy.status != MultisigStatus::Funding {
                    // TODO: Create distinct error type for legacy check
                    return Err(Error::LifecycleMustBeFunded);
                }

                client.insert_attested_multisig(legacy_id, legacy)?;
            }
            _ => {}
        }

        let submitter = MultisigSubmitter { queue: tx };

        let this = MultisigManager {
            queue: Arc::new(Mutex::new(rx)),
            submitter: submitter.clone(),
            client,
        };

        Ok((this, submitter))
    }
    /// Registers a multisig as staging for an upcoming DKG ceremony.
    ///
    /// If the multisig has already been attested, respectively it completed DKG
    /// and transitioned past staging, then this is a no-op that returns
    /// `Ok(true)`. Otherwise the entry is persisted as [`StagedMultisigEntry`]
    /// and `Ok(false)` is returned.
    pub fn set_staging_multisig(
        &self,
        multisig_id: MultisigId,
        coordinator: frost::Identifier,
        fed_members: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
    ) -> Result<bool, Error> {
        // Check if the given multisig has already been attested.
        if self.client.get_attested_multisig(multisig_id)?.is_some() {
            return Ok(true);
        }

        let staging =
            StagedMultisigEntry { multisig_id, coordinator, fed_members };

        self.client.insert_staging_multisig(multisig_id, staging)?;

        Ok(false)
    }
    /// Returns a cloned handle to the [`MultisigSubmitter`] for this manager.
    pub fn submitter(&self) -> MultisigSubmitter {
        self.submitter.clone()
    }
    /// Returns all attested multisigs that can receive pegins. This excludes
    /// staging, sunset and expired (removed) multisigs.
    pub fn get_active_multisigs(
        &self,
    ) -> Result<Vec<AttestedMultisigEntry>, Error> {
        self.client.get_active_multisigs().map_err(Into::into)
    }
    /// Returns the single attested multisig that should receive new pegins.
    pub fn get_funding_multisig(
        &self,
    ) -> Result<Option<AttestedMultisigEntry>, Error> {
        // NOTE: Linear scan is acceptable here since the total number of
        // multisig records is tiny in practice (one or two).
        let funding: Vec<_> = self
            .get_active_multisigs()?
            .into_iter()
            .filter(|m| m.status == MultisigStatus::Funding)
            .collect();

        if funding.len() > 1 {
            return Err(Error::MultipleFundingMultisigs);
        }

        Ok(funding.into_iter().nth(0))
    }
    /// Polls for a pending message to propose to consensus.
    ///
    /// Dequeues the next submission from clients, validates it via dry-run, and
    /// notifies the submitter of the result via their callback. Returns
    /// `Some(message)` if validation passed (ready for consensus proposal), or
    /// `None` if the queue is empty or validation failed.
    ///
    /// This is the "outbound" half of the consensus interface.
    pub fn send(&self) -> Option<Message> {
        let l = self.queue.lock().expect("poisoned lock");
        let Ok(payload) = l.try_recv() else {
            return None;
        };
        std::mem::drop(l);

        // Do a dry-run on the message payloads. This ensures that we don't
        // accidentally propose invalid messages to the consensus layer.
        let validate_messages = || match payload.message.clone() {
            Message::Attestation {
                multisig_id,
                public_key_package,
                signing_package,
                signatures,
            } => self.validate_attestation_dry_run(
                multisig_id,
                public_key_package,
                signing_package,
                signatures,
            ),
            Message::Sunsetting {
                multisig_id, //
                coordinator_signature,
            } => todo!(),
            Message::Expiration {
                multisig_id, //
                coordinator_signature,
            } => self.validate_expiration_dry_run(
                multisig_id,
                coordinator_signature,
            ),
        };

        let res = validate_messages();
        let res_is_ok = res.is_ok();

        // Send the result back to the caller; we just ignore if the caller
        // already hung-up.
        let _ = payload.callback.send(res);

        if res_is_ok {
            Some(payload.message)
        } else {
            None
        }
    }
    /// Applies a consensus-committed message to update local state.
    ///
    /// Validates and executes the message: attestations transition multisigs
    /// from Staged to Active, expirations remove Sunset multisigs entirely.
    ///
    /// This is the "inbound" half of the consensus interface.
    pub fn recv(&self, msg: Message) -> Result<(), Error> {
        match msg {
            Message::Attestation {
                multisig_id,
                public_key_package,
                signing_package,
                signatures,
            } => self.validate_attestation(
                multisig_id,
                public_key_package,
                signing_package,
                signatures,
            ),
            Message::Sunsetting {
                multisig_id, //
                coordinator_signature,
            } => self.validate_sunsetting(
                multisig_id, //
                coordinator_signature,
            ),
            Message::Expiration {
                multisig_id, //
                coordinator_signature,
            } => self.validate_expiration(
                multisig_id, //
                coordinator_signature,
            ),
        }
    }
    /// Validates a FROST attestation without modifying state.
    ///
    /// Verifies that all federation members correctly participated in the DKG
    /// ceremony by checking each member's FROST signature share and attestation
    /// signature, then aggregating into the final group signature.
    fn validate_attestation_dry_run(
        &self,
        multisig_id: MultisigId,
        public_key_package: PublicKeyPackage,
        signing_package: frost::SigningPackage,
        signatures: BTreeMap<
            frost::Identifier,
            (frost::round2::SignatureShare, secp256k1::ecdsa::Signature),
        >,
    ) -> Result<(), Error> {
        let staging = self
            .client
            .get_staging_multisig(multisig_id)?
            // TODO: This should distinguish between non-existing ID and non-Staged.
            .ok_or(Error::LifecycleMustBeStaged)?;

        let coordinator = staging.coordinator;
        let fed_members = staging.fed_members;

        debug_assert!(fed_members.contains_key(&coordinator));

        let mut m = AttestationManager::new(
            multisig_id,
            fed_members,
            signing_package,
            public_key_package,
        )
        .unwrap();

        // Verify each member's FROST signature share and attestation signature.
        for (frost_id, (sig_share, att_sig)) in signatures {
            m.validate_signature_share(frost_id, sig_share, att_sig)?;
        }

        // Aggregate shares and verify the group signature against the public key.
        let _aggr_sig = m.finalize()?;

        Ok(())
    }
    /// Validates a FROST attestation and transitions the multisig to Active.
    ///
    /// Verifies that all federation members correctly participated in the DKG
    /// ceremony, then promotes the multisig from Staged to Active with the
    /// verified public key package.
    fn validate_attestation(
        &self,
        multisig_id: MultisigId,
        public_key_package: PublicKeyPackage,
        signing_package: frost::SigningPackage,
        signatures: BTreeMap<
            frost::Identifier,
            (frost::round2::SignatureShare, secp256k1::ecdsa::Signature),
        >,
    ) -> Result<(), Error> {
        self.validate_attestation_dry_run(
            multisig_id,
            public_key_package.clone(),
            signing_package,
            signatures,
        )?;

        // Retrieve staging multisig entry.
        let staging = self
            .client
            .get_staging_multisig(multisig_id)?
            .ok_or(Error::LifecycleMustBeStaged)?;

        // Demote the current funding multisig to the Degrading state, if one exists.
        if let Some(mut degrading) = self.get_funding_multisig()? {
            if degrading.status != MultisigStatus::Funding {
                // This is technically a sanity check; this should never occur.
                return Err(Error::LifecycleMustBeFunded);
            }

            // Update status.
            degrading.status = MultisigStatus::Degrading;
            self.client
                .insert_attested_multisig(degrading.multisig_id, degrading)?;
        }

        // Promote the current staging multisig to the Funding state.
        {
            let funding = AttestedMultisigEntry {
                multisig_id,
                coordinator: staging.coordinator,
                fed_members: staging.fed_members,
                public_key_package,
                status: MultisigStatus::Funding,
            };

            // Update status.
            self.client.insert_attested_multisig(multisig_id, funding)?;
            debug_assert!(self
                .client
                .get_staging_multisig(multisig_id)?
                .is_none());
        }

        Ok(())
    }

    /// Verifies a coordinator's signature over a commitment built by the provided closure.
    ///
    /// The closure should build and return the 32-byte commitment to be signed.
    // TODO: Deprecate this => should be embedded
    fn verify_coordinator_signature<F>(
        coord_pubkey: &secp256k1::PublicKey,
        coordinator_signature: &secp256k1::ecdsa::Signature,
        build_commitment: F,
    ) -> Result<(), Error>
    where
        F: FnOnce() -> Result<[u8; 32], Error>,
    {
        let commit = build_commitment()?;

        let msg =
            secp256k1::Message::from_digest_slice(&commit).expect("valid size");

        let secp = secp256k1::Secp256k1::new();
        coord_pubkey
            .verify(&secp, &msg, coordinator_signature)
            .map_err(|_| Error::SignatureVerificationFailed)?;

        Ok(())
    }
    fn validate_sunsetting(
        &self,
        multisig_id: MultisigId,
        coordinator_signature: secp256k1::ecdsa::Signature,
    ) -> Result<(), Error> {
        // Only degrading multisigs can be sunset.
        let degrading = self
            .client
            .get_attested_multisig(multisig_id)?
            .ok_or(Error::LifecycleMustBeDegraded)?;

        if degrading.status != MultisigStatus::Degrading {
            return Err(Error::LifecycleMustBeDegraded);
        }

        let coord_pubkey = degrading
            .fed_members
            .get(&degrading.coordinator)
            .expect("coordinator pubkey must exist");

        Self::verify_coordinator_signature(
            coord_pubkey,
            &coordinator_signature,
            || {
                let mut commit = [0; 32];
                let mut t = Transcript::new(b"botanix/multisig-sunsetting/v1");
                t.append_u64(b"multisig_id", multisig_id.as_u32() as u64);
                t.append_message(
                    b"public_key_package",
                    &degrading.public_key_package.serialize()?,
                );
                t.challenge_bytes(b"sunsetting_commit", &mut commit);
                Ok(commit)
            },
        )?;

        let mut degrading = degrading;

        degrading.status = MultisigStatus::Sunsetting;
        self.client.insert_attested_multisig(multisig_id, degrading)?;

        Ok(())
    }
    /// Validates an expiration request without modifying state.
    ///
    /// Verifies the coordinator's signature over a commitment binding the
    /// multisig ID and public key package.
    fn validate_expiration_dry_run(
        &self,
        multisig_id: MultisigId,
        coordinator_signature: secp256k1::ecdsa::Signature,
    ) -> Result<(), Error> {
        // Only Sunset multisigs can be expired.
        let sunset = self
            .client
            .get_attested_multisig(multisig_id)?
            .ok_or(Error::LifecycleMustBeSunset)?;

        if sunset.status != MultisigStatus::Sunsetting {
            return Err(Error::LifecycleMustBeSunset);
        }

        let coord_pubkey = sunset
            .fed_members
            .get(&sunset.coordinator)
            .expect("coordinator pubkey must exist");

        Self::verify_coordinator_signature(
            coord_pubkey,
            &coordinator_signature,
            || {
                let mut commit = [0; 32];
                let mut t = Transcript::new(b"botanix/multisig-expiration/v1");
                t.append_u64(b"multisig_id", multisig_id.as_u32() as u64);
                t.append_message(
                    b"public_key_package",
                    &sunset.public_key_package.serialize()?,
                );
                t.challenge_bytes(b"expiration_commit", &mut commit);
                Ok(commit)
            },
        )
    }
    /// Validates an expiration request and removes the multisig.
    ///
    /// Finalizes the sunset process by verifying the coordinator's signature,
    /// then removing the multisig entirely. Only the designated coordinator can
    /// authorize expiration, and only for multisigs in the Sunset state.
    fn validate_expiration(
        &self,
        multisig_id: MultisigId,
        coordinator_signature: secp256k1::ecdsa::Signature,
    ) -> Result<(), Error> {
        self.validate_expiration_dry_run(multisig_id, coordinator_signature)?;

        let did_remove = self.client.remove_multisig(multisig_id)?;
        debug_assert!(did_remove);

        Ok(())
    }
}

// TODO (lamafab): This is essentially copied from the `dkg` module in the
// `botanix-btc-server` crate. The DKG module should be its own crate, so that
// this crate can just import the functionality as a dependency.
struct AttestationManager {
    secp: secp256k1::Secp256k1<secp256k1::All>,
    transcript: Transcript,
    fed_members: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
    signing_package: frost::SigningPackage,
    public_key_package: frost::keys::PublicKeyPackage,
    signature_shares:
        BTreeMap<frost::Identifier, frost::round2::SignatureShare>,
}

impl std::fmt::Debug for AttestationManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttestationManager")
            .field("transcript", &"[REDACTED]")
            .field("fed_members", &self.fed_members)
            .field("signing_package", &self.signing_package)
            .field("public_key_package", &self.public_key_package)
            .field("signature_shares", &self.signature_shares)
            .finish()
    }
}

impl AttestationManager {
    fn new(
        multisig_id: MultisigId,
        fed_members: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
        signing_package: frost::SigningPackage,
        public_key_package: frost::keys::PublicKeyPackage,
    ) -> Result<Self, Error> {
        let mut commit = [0; 32];

        let mut t = Transcript::new(b"botanix/multisig-attestation/v1");
        t.append_u64(b"multisig_id", multisig_id.as_u32() as u64);
        t.append_message(b"signing_package", &signing_package.serialize()?);
        t.append_message(
            b"public_key_package",
            &public_key_package.serialize()?,
        );
        t.challenge_bytes(b"attestation_commit", &mut commit);

        let secp = secp256k1::Secp256k1::new();

        Ok(AttestationManager {
            secp,
            transcript: t,
            fed_members,
            signing_package,
            public_key_package,
            signature_shares: BTreeMap::new(),
        })
    }
    fn validate_signature_share(
        &mut self,
        id: frost::Identifier,
        signature_share: frost::round2::SignatureShare,
        attestation_sig: secp256k1::ecdsa::Signature,
    ) -> Result<(), Error> {
        let mut commit = [0; 32];
        let fed = self.fed_members.get(&id).ok_or(Error::NotAFedMember)?;

        {
            // Clone the transcript, which is then dropped after the challenge
            // bytes have been generated. Notably, this ties the signing
            // challenge to the DKG process itself.
            let mut t = self.transcript.clone();
            t.append_message(b"signature_share", &signature_share.serialize());
            t.challenge_bytes(b"attestation_commit", &mut commit);
        }

        let msg =
            secp256k1::Message::from_digest_slice(&commit).expect("valid size");
        fed.verify(&self.secp, &msg, &attestation_sig)
            .map_err(|_| Error::SignatureVerificationFailed)?;

        self.signature_shares.insert(id, signature_share);

        Ok(())
    }
    fn finalize(&mut self) -> Result<frost::Signature, Error> {
        if self.signature_shares.len() != self.fed_members.len() {
            return Err(Error::NotEnoughSignatureShares);
        }

        // Verify each participants signature share and aggregate the final
        // signature which is then verified against the aggregated public key.
        let aggr_sig = frost::aggregate(
            &self.signing_package,
            &self.signature_shares,
            &self.public_key_package,
        )?;

        self.public_key_package
            .verifying_key()
            .verify(self.signing_package.message(), &aggr_sig)?;

        Ok(aggr_sig)
    }
}

/// Returns the hard-coded legacy multisig for the Botanix mainnet.
pub fn attested_legacy_multisig() -> AttestedMultisigEntry {
    const PUBLIC_KEY_PACKAGE: &[u8] = &[
        0, 35, 15, 138, 179, 16, 12, 223, 219, 150, 131, 74, 116, 233, 236,
        196, 96, 205, 96, 130, 121, 192, 88, 21, 198, 81, 210, 88, 18, 92, 42,
        123, 15, 91, 242, 12, 143, 253, 3, 77, 19, 128, 136, 125, 48, 26, 49,
        127, 242, 77, 108, 244, 245, 130, 85, 231, 115, 29, 160, 96, 12, 120,
        236, 65, 200, 30, 202, 128, 136, 246, 99, 26, 78, 84, 136, 74, 83, 221,
        62, 220, 217, 71, 74, 190, 96, 197, 43, 35, 160, 54, 176, 6, 100, 35,
        87, 32, 18, 106, 2, 218, 56, 7, 236, 2, 179, 216, 87, 238, 238, 15,
        111, 24, 119, 83, 28, 239, 120, 142, 103, 150, 124, 62, 180, 218, 0,
        169, 67, 216, 139, 217, 50, 59, 81, 21, 100, 15, 29, 28, 169, 221, 254,
        245, 163, 73, 165, 195, 19, 121, 156, 235, 162, 213, 99, 80, 154, 254,
        130, 153, 51, 44, 16, 98, 167, 108, 22, 243, 75, 70, 2, 22, 158, 167,
        183, 127, 218, 231, 218, 70, 166, 117, 26, 109, 150, 251, 153, 63, 51,
        241, 68, 61, 22, 149, 52, 10, 228, 247, 128, 243, 9, 218, 212, 52, 39,
        160, 118, 176, 185, 222, 25, 54, 213, 2, 171, 233, 2, 75, 75, 154, 59,
        199, 10, 16, 208, 24, 249, 238, 56, 171, 146, 37, 245, 114, 93, 3, 59,
        200, 105, 107, 16, 116, 48, 126, 51, 174, 127, 189, 216, 86, 241, 31,
        166, 225, 122, 26, 217, 255, 139, 34, 65, 23, 166, 231, 117, 105, 208,
        10, 58, 249, 110, 139, 98, 168, 251, 48, 220, 230, 203, 109, 98, 62,
        237, 156, 94, 173, 105, 207, 125, 55, 153, 106, 26, 137, 241, 53, 153,
        182, 171, 115, 3, 220, 240, 214, 195, 184, 98, 53, 60, 194, 145, 50,
        79, 239, 211, 122, 126, 5, 137, 135, 228, 154, 185, 70, 230, 224, 11,
        105, 134, 66, 235, 214, 127, 81, 135, 19, 75, 111, 151, 67, 66, 101,
        73, 251, 9, 148, 85, 15, 88, 125, 54, 232, 200, 208, 115, 117, 45, 33,
        10, 64, 197, 111, 246, 75, 106, 2, 69, 117, 200, 220, 137, 3, 49, 130,
        36, 214, 68, 139, 194, 253, 66, 129, 196, 58, 175, 134, 49, 56, 76, 79,
        247, 112, 244, 74, 162, 178, 195, 154, 109, 91, 187, 63, 86, 64, 106,
        229, 176, 35, 33, 147, 138, 205, 251, 95, 148, 146, 194, 28, 67, 54,
        243, 67, 95, 201, 128, 135, 169, 20, 169, 14, 3, 53, 69, 28, 118, 152,
        117, 204, 254, 231, 164, 157, 118, 53, 224, 80, 149, 226, 161, 30, 86,
        111, 189, 164, 48, 118, 57, 222, 27, 3, 41, 32, 247, 110, 251, 187,
        176, 203, 148, 245, 26, 141, 181, 107, 93, 7, 206, 147, 85, 201, 178,
        5, 208, 23, 201, 64, 134, 174, 5, 64, 127, 27, 5, 34, 110, 3, 203, 63,
        117, 212, 230, 144, 222, 136, 255, 169, 68, 209, 191, 237, 216, 151,
        129, 54, 193, 218, 81, 15, 249, 207, 247, 244, 106, 50, 42, 123, 10,
        168, 141, 16, 170, 236, 168, 61, 75, 204, 13, 184, 143, 145, 151, 157,
        75, 18, 206, 8, 254, 226, 106, 107, 46, 177, 66, 145, 207, 98, 13, 188,
        132, 203, 3, 72, 212, 29, 94, 7, 9, 144, 139, 43, 13, 47, 19, 236, 67,
        153, 126, 222, 197, 12, 206, 199, 35, 154, 105, 35, 19, 147, 187, 23,
        220, 220, 91, 155, 145, 133, 62, 186, 237, 86, 188, 16, 238, 14, 112,
        134, 151, 148, 80, 36, 55, 251, 172, 7, 174, 114, 79, 229, 185, 213,
        30, 3, 78, 38, 104, 3, 134, 11, 219, 115, 29, 94, 202, 211, 224, 199,
        223, 68, 122, 49, 20, 210, 21, 247, 200, 139, 75, 244, 123, 96, 53,
        157, 243, 19, 222, 62, 188, 22, 158, 88, 249, 63, 61, 30, 97, 250, 150,
        158, 190, 7, 32, 39, 199, 51, 206, 86, 127, 162, 81, 149, 208, 130, 13,
        166, 136, 165, 152, 222, 24, 14, 2, 70, 165, 120, 168, 44, 9, 48, 5,
        213, 115, 108, 105, 17, 138, 62, 7, 170, 255, 92, 42, 223, 178, 85,
        159, 27, 205, 70, 214, 12, 34, 136, 130, 172, 197, 159, 249, 40, 76,
        205, 49, 208, 14, 123, 169, 145, 252, 96, 128, 142, 96, 26, 2, 128, 79,
        6, 59, 90, 29, 133, 161, 26, 217, 244, 230, 2, 194, 40, 183, 215, 87,
        200, 127, 172, 46, 53, 122, 41, 204, 70, 41, 154, 210, 37, 23, 155,
        185, 100, 15, 33, 74, 214, 43, 152, 89, 69, 69, 72, 208, 194, 249, 172,
        249, 60, 221, 80, 243, 108, 104, 89, 171, 32, 138, 78, 108, 78, 188,
        151, 72, 251, 203, 155, 162, 145, 78, 117, 23, 5, 73, 88, 2, 179, 155,
        35, 79, 14, 165, 223, 186, 170, 22, 22, 84, 191, 65, 25, 57, 178, 180,
        181, 182, 165, 208, 190, 96, 164, 11, 32, 224, 57, 36, 177, 182, 227,
        229, 106, 191, 189, 12, 77, 117, 47, 65, 107, 161, 92, 109, 135, 63,
        209, 194, 39, 53, 224, 126, 236, 3, 127, 134, 236, 66, 173, 196, 212,
        170, 2, 252, 120, 234, 132, 29, 227, 165, 160, 57, 112, 155, 165, 8,
        165, 95, 127, 27, 242, 25, 203, 83, 5, 154, 114, 219, 244, 192, 200,
        105, 167, 212, 228, 229, 115, 185, 195, 155, 219, 62, 36, 55, 223, 89,
        198, 165, 161, 144, 223, 113, 163, 225, 246, 116, 254, 167, 122, 7,
        190, 120, 125, 41, 67, 171, 129, 2, 143, 98, 147, 228, 89, 117, 85,
        232, 246, 86, 243, 109, 238, 248, 201, 92, 56, 89, 66, 129, 155, 29,
        153, 43, 47, 81, 71, 153, 234, 187, 92, 115, 247, 137, 46, 56, 22, 240,
        39, 158, 129, 33, 141, 238, 65, 83, 155, 62, 82, 218, 191, 252, 150,
        228, 42, 27, 100, 213, 224, 160, 158, 100, 63, 157, 2, 178, 125, 134,
        208, 173, 14, 148, 141, 93, 104, 12, 106, 0, 177, 211, 91, 226, 250,
        157, 33, 245, 102, 85, 80, 175, 44, 52, 67, 181, 253, 165, 127, 3, 174,
        38, 246, 21, 46, 250, 110, 101, 97, 159, 67, 106, 174, 80, 118, 53,
        108, 172, 171, 151, 190, 209, 12, 41, 74, 56, 183, 119, 239, 166, 110,
        114,
    ];

    let coordinator = authority_index_to_frost_identifier(0);

    let fed_member_keys: &[&str] = &[
        "02d124f7dfd25575db9a41e658ef67e0e2f7ea65c5d21307fb0c8c56c64d2e88cc",
        "02864035c66736091d2980b853f547a5031c68cab9fb667971ff33c24c90a8a7ef",
        "02224bac407130ae5bd0074f4abebc7ae8dfea5f4917ef43cf2ac19dce8f8b7d9f",
        "023da6efd68923bc779aa0bafa67fbff8752884f88cde4af8289056a5862539355",
        "037b8bd0bfe8f068dec1c6481653f6b2196ec287273fe13b81ed18626fbbf14e8a",
        "02c46c5a797a99543e0302376859e95f2e4855b644d94842094ea1d6c926fb1049",
        "0387dd207bc1b67c37b8fed06b79f3f51d65e8687fbeed62e9224429d3a2713692",
        "036c44b3867d303438438e0d0fc1888f8f7915e1d19cecbc6f26f7e4e8189d4145",
        "033be40a778603c9045894576ba34f178cb4563e50b19efa7c5766522a5a043e8e",
        "03f900e2e8596442966fcaaecc4274b50ce4389895e9e1b33925f94a16e0f4a4cb",
        "03ed0f6c518e6adcae9e323aeb8cbd2a30ad5e3f432db73a11393467feb73aad0e",
        "03f9c5a09a42e93d63af0c5843a8b971d2d54130835497b2bb39ec5b36a3c540ee",
        "022054f934fb0e787891d80475b55d4a206eb08f48b5506167fca72f474e87e971",
        "037088807dea373f557dcde1b5d8bca4bf31dcf3144f67af80861501d3a5c881b8",
        "0336cb3792c5ccdeb6899e93629e35bdec9079b09c8b451f0e30b12005b334b963",
        "039ef3cc23c3f37d8b825a53157991fc9e36d20ad58300aa9fb499d885e9e1b744",
    ];

    let fed_members: BTreeMap<frost::Identifier, secp256k1::PublicKey> =
        fed_member_keys
            .iter()
            .enumerate()
            .map(|(i, hex_key)| {
                let id = authority_index_to_frost_identifier(i as u16);
                let pubkey = secp256k1::PublicKey::from_str(hex_key)
                    .expect("valid public key");
                (id, pubkey)
            })
            .collect();

    let public_key_package =
        frost::keys::PublicKeyPackage::deserialize(PUBLIC_KEY_PACKAGE)
            .expect("valid public key package");

    AttestedMultisigEntry {
        multisig_id: LEGACY_MULTISIG_ID,
        coordinator,
        fed_members,
        public_key_package,
        status: MultisigStatus::Funding,
    }
}

/// Pre-built FROST key material and helpers for tests.
#[cfg(test)]
pub mod test_data {
    use super::*;

    /// Alice's serialized [`secp256k1::SecretKey`].
    const ALICE_SECRET_KEY: &[u8] = &[
        146, 225, 40, 141, 15, 119, 60, 106, 253, 37, 64, 178, 200, 220, 195,
        201, 4, 19, 248, 96, 255, 186, 154, 73, 83, 41, 46, 163, 134, 113, 209,
        35,
    ];

    /// Alice's serialized [`secp256k1::PublicKey`].
    const ALICE_PUBLIC_KEY: &[u8] = &[
        3, 108, 249, 52, 233, 225, 4, 168, 61, 13, 17, 6, 202, 193, 195, 223,
        178, 137, 195, 42, 201, 102, 37, 233, 223, 233, 229, 101, 201, 52, 199,
        162, 211,
    ];

    /// Bob's serialized [`secp256k1::PublicKey`].
    const BOB_PUBLIC_KEY: &[u8] = &[
        3, 73, 11, 12, 91, 6, 179, 236, 32, 88, 44, 124, 244, 9, 105, 155, 209,
        31, 231, 74, 91, 21, 149, 228, 223, 190, 160, 163, 191, 77, 122, 247,
        176,
    ];

    /// Dave's serialized [`secp256k1::PublicKey`].
    const DAVE_PUBLIC_KEY: &[u8] = &[
        3, 107, 180, 161, 112, 218, 234, 60, 196, 92, 211, 52, 202, 252, 39,
        253, 134, 72, 50, 161, 213, 49, 107, 139, 124, 255, 195, 150, 39, 65,
        151, 89, 142,
    ];

    /// A valid serialized [`PublicKeyPackage`] for use in tests.
    const PUBLIC_KEY_PACKAGE: &[u8] = &[
        0, 35, 15, 138, 179, 3, 12, 223, 219, 150, 131, 74, 116, 233, 236, 196,
        96, 205, 96, 130, 121, 192, 88, 21, 198, 81, 210, 88, 18, 92, 42, 123,
        15, 91, 242, 12, 143, 253, 3, 141, 160, 148, 22, 247, 151, 219, 172,
        146, 158, 228, 17, 245, 203, 124, 4, 167, 105, 78, 250, 83, 146, 77,
        246, 134, 20, 178, 184, 128, 79, 126, 86, 52, 39, 160, 118, 176, 185,
        222, 25, 54, 213, 2, 171, 233, 2, 75, 75, 154, 59, 199, 10, 16, 208,
        24, 249, 238, 56, 171, 146, 37, 245, 114, 93, 3, 161, 206, 221, 31,
        199, 87, 253, 42, 238, 234, 227, 138, 203, 117, 193, 68, 174, 197, 241,
        59, 201, 245, 11, 128, 142, 51, 68, 43, 211, 116, 247, 222, 172, 197,
        159, 249, 40, 76, 205, 49, 208, 14, 123, 169, 145, 252, 96, 128, 142,
        96, 26, 2, 128, 79, 6, 59, 90, 29, 133, 161, 26, 217, 244, 230, 2, 251,
        221, 172, 123, 86, 36, 91, 207, 1, 171, 37, 164, 227, 107, 69, 50, 206,
        122, 18, 175, 1, 11, 13, 122, 179, 70, 252, 91, 223, 16, 242, 203, 2,
        46, 184, 148, 111, 35, 88, 118, 187, 157, 248, 249, 236, 149, 179, 177,
        117, 154, 9, 13, 235, 38, 164, 113, 14, 65, 160, 37, 49, 240, 242, 164,
        62,
    ];

    /// A valid serialized [`frost::SigningPackage`] for use in tests.
    const SIGNING_PACKAGE: &[u8] = &[
        0, 35, 15, 138, 179, 3, 12, 223, 219, 150, 131, 74, 116, 233, 236, 196,
        96, 205, 96, 130, 121, 192, 88, 21, 198, 81, 210, 88, 18, 92, 42, 123,
        15, 91, 242, 12, 143, 253, 0, 35, 15, 138, 179, 3, 161, 102, 220, 50,
        174, 161, 17, 197, 168, 201, 2, 168, 45, 12, 123, 29, 95, 3, 142, 182,
        83, 149, 95, 78, 71, 247, 2, 134, 89, 116, 124, 60, 2, 235, 116, 142,
        89, 226, 151, 123, 121, 86, 124, 165, 175, 98, 105, 96, 70, 165, 207,
        136, 140, 66, 133, 217, 9, 126, 169, 120, 226, 228, 154, 198, 249, 52,
        39, 160, 118, 176, 185, 222, 25, 54, 213, 2, 171, 233, 2, 75, 75, 154,
        59, 199, 10, 16, 208, 24, 249, 238, 56, 171, 146, 37, 245, 114, 93, 0,
        35, 15, 138, 179, 2, 216, 215, 115, 163, 175, 106, 16, 53, 57, 14, 15,
        165, 146, 133, 145, 123, 21, 25, 108, 187, 18, 133, 59, 14, 146, 185,
        56, 54, 175, 110, 208, 31, 3, 180, 95, 46, 200, 36, 101, 62, 201, 39,
        100, 193, 156, 184, 153, 2, 94, 63, 123, 88, 154, 194, 253, 74, 24,
        220, 38, 79, 205, 40, 185, 179, 248, 172, 197, 159, 249, 40, 76, 205,
        49, 208, 14, 123, 169, 145, 252, 96, 128, 142, 96, 26, 2, 128, 79, 6,
        59, 90, 29, 133, 161, 26, 217, 244, 230, 0, 35, 15, 138, 179, 3, 240,
        123, 108, 79, 215, 64, 247, 76, 91, 62, 31, 213, 169, 33, 206, 95, 246,
        77, 31, 65, 197, 250, 234, 166, 218, 166, 210, 210, 100, 77, 175, 187,
        3, 208, 12, 159, 223, 154, 165, 137, 26, 20, 131, 246, 185, 106, 194,
        178, 197, 95, 70, 202, 12, 128, 164, 250, 244, 105, 213, 199, 141, 116,
        228, 153, 37, 32, 126, 32, 153, 117, 58, 25, 235, 74, 173, 59, 239,
        131, 182, 70, 169, 62, 196, 156, 180, 176, 134, 248, 159, 251, 56, 133,
        178, 254, 215, 109, 109, 6,
    ];

    /// Alice's serialized [`frost::round2::SignatureShare`].
    const ALICE_ATTEST_SIGNATURE_SHARE: &[u8] = &[
        149, 56, 18, 64, 255, 195, 153, 40, 104, 57, 27, 33, 26, 226, 12, 158,
        49, 79, 73, 233, 147, 124, 0, 134, 142, 171, 208, 114, 7, 71, 29, 173,
    ];

    /// Alice's serialized [`secp256k1::ecdsa::Signature`].
    const ALICE_ATTEST_SIGNATURE: &[u8] = &[
        254, 154, 170, 192, 254, 114, 173, 7, 179, 138, 17, 126, 198, 22, 22,
        83, 93, 242, 32, 202, 239, 204, 125, 73, 55, 36, 184, 144, 40, 134,
        163, 166, 117, 39, 244, 222, 150, 48, 187, 144, 253, 103, 56, 194, 40,
        168, 208, 41, 31, 165, 78, 133, 224, 106, 42, 197, 30, 222, 145, 23,
        222, 65, 2, 27,
    ];

    /// Bob's serialized [`frost::round2::SignatureShare`].
    const BOB_ATTEST_SIGNATURE_SHARE: &[u8] = &[
        210, 36, 166, 114, 238, 122, 47, 226, 172, 6, 45, 71, 93, 163, 78, 203,
        190, 248, 12, 131, 49, 208, 46, 254, 160, 54, 254, 86, 133, 220, 147,
        130,
    ];

    /// Bob's serialized [`secp256k1::ecdsa::Signature`].
    const BOB_ATTEST_SIGNATURE: &[u8] = &[
        197, 248, 223, 62, 61, 3, 10, 77, 153, 149, 146, 105, 112, 0, 116, 170,
        120, 109, 153, 151, 99, 219, 118, 31, 41, 152, 109, 249, 219, 20, 206,
        44, 24, 133, 48, 31, 182, 107, 167, 67, 164, 247, 214, 211, 61, 11, 92,
        105, 144, 102, 46, 134, 51, 183, 179, 138, 144, 235, 183, 149, 99, 181,
        13, 12,
    ];

    /// Dave's serialized [`frost::round2::SignatureShare`].
    const DAVE_ATTEST_SIGNATURE_SHARE: &[u8] = &[
        52, 123, 128, 38, 28, 78, 122, 234, 29, 76, 96, 198, 26, 182, 87, 162,
        238, 182, 183, 117, 224, 14, 90, 143, 70, 55, 176, 29, 75, 207, 3, 184,
    ];

    /// Dave's serialized [`secp256k1::ecdsa::Signature`].
    const DAVE_ATTEST_SIGNATURE: &[u8] = &[
        97, 147, 4, 99, 175, 163, 134, 189, 84, 75, 176, 215, 228, 150, 166,
        52, 54, 143, 194, 27, 143, 179, 4, 145, 127, 106, 34, 96, 129, 100, 71,
        31, 34, 99, 251, 125, 157, 122, 232, 19, 121, 174, 144, 105, 107, 183,
        155, 202, 231, 66, 180, 37, 182, 210, 158, 14, 138, 40, 120, 52, 178,
        238, 81, 147,
    ];

    /// Eve's serialized [`frost::round2::SignatureShare`].
    const EVE_ATTEST_SIGNATURE_SHARE: &[u8] = &[
        100, 52, 149, 157, 149, 140, 39, 68, 80, 63, 185, 95, 113, 77, 18, 144,
        128, 155, 150, 16, 252, 3, 130, 172, 81, 219, 102, 61, 178, 86, 164,
        197,
    ];

    /// Eve's serialized [`secp256k1::ecdsa::Signature`].
    const EVE_ATTEST_SIGNATURE: &[u8] = &[
        31, 227, 54, 41, 251, 137, 91, 71, 155, 115, 75, 248, 49, 217, 79, 176,
        216, 87, 89, 210, 147, 195, 40, 176, 17, 206, 135, 234, 146, 230, 203,
        56, 4, 130, 112, 202, 157, 151, 229, 73, 101, 134, 235, 149, 63, 10,
        81, 94, 150, 43, 208, 133, 108, 199, 26, 243, 37, 59, 244, 18, 53, 183,
        62, 82,
    ];

    /// The multisig ID that the test-data signatures have attested for.
    pub const UPGRADE_MULTISIG: MultisigId = MultisigId::new(99);

    /// The legacy multisig ID constant (pre-dynafed).
    pub const LEGACY_MULTISIG: MultisigId = MultisigId::LEGACY;

    /// Alice's FROST identifier (derived from `0u16`).
    pub fn alice_id() -> frost::Identifier {
        authority_index_to_frost_identifier(0)
    }

    /// Bob's FROST identifier (derived from `1u16`).
    pub fn bob_id() -> frost::Identifier {
        authority_index_to_frost_identifier(1)
    }

    /// Dave's FROST identifier (derived from `2u16`).
    pub fn dave_id() -> frost::Identifier {
        authority_index_to_frost_identifier(2)
    }

    /// Returns a legacy attested multisig in `Funding` status for migration tests.
    pub fn preload_legacy() -> AttestedMultisigEntry {
        let coordinator = alice_id();
        let fed_members = fed_members();
        debug_assert!(fed_members.contains_key(&coordinator));

        AttestedMultisigEntry {
            multisig_id: LEGACY_MULTISIG,
            coordinator,
            fed_members,
            public_key_package: public_key_package(),
            status: MultisigStatus::Funding,
        }
    }

    /// Deserializes the hardcoded [`frost::SigningPackage`].
    pub fn signing_package() -> frost::SigningPackage {
        frost::SigningPackage::deserialize(SIGNING_PACKAGE).unwrap()
    }

    /// Deserializes the hardcoded [`PublicKeyPackage`].
    pub fn public_key_package() -> PublicKeyPackage {
        PublicKeyPackage::deserialize(PUBLIC_KEY_PACKAGE).unwrap()
    }

    /// Builds the [`Message::Attestation`] for [`UPGRADE_MULTISIG`] using
    /// the hardcoded DKG test data (public key package, signing package,
    /// and all three members' signature shares and attestation signatures).
    pub fn attestation_message() -> Message {
        Message::Attestation {
            multisig_id: UPGRADE_MULTISIG,
            public_key_package: public_key_package(),
            signing_package: signing_package(),
            signatures: BTreeMap::from([
                (
                    alice_id(),
                    (alice_attest_signature_share(), alice_attest_signature()),
                ),
                (
                    bob_id(),
                    (bob_attest_signature_share(), bob_attest_signature()),
                ),
                (
                    dave_id(),
                    (dave_attest_signature_share(), dave_attest_signature()),
                ),
            ]),
        }
    }

    /// Deserializes Alice's [`frost::round2::SignatureShare`].
    pub fn alice_attest_signature_share() -> frost::round2::SignatureShare {
        frost::round2::SignatureShare::deserialize(ALICE_ATTEST_SIGNATURE_SHARE)
            .unwrap()
    }

    /// Deserializes Alice's [`secp256k1::ecdsa::Signature`].
    pub fn alice_attest_signature() -> secp256k1::ecdsa::Signature {
        secp256k1::ecdsa::Signature::from_compact(ALICE_ATTEST_SIGNATURE)
            .unwrap()
    }

    /// Deserializes Bob's [`frost::round2::SignatureShare`].
    pub fn bob_attest_signature_share() -> frost::round2::SignatureShare {
        frost::round2::SignatureShare::deserialize(BOB_ATTEST_SIGNATURE_SHARE)
            .unwrap()
    }

    /// Deserializes Bob's [`secp256k1::ecdsa::Signature`].
    pub fn bob_attest_signature() -> secp256k1::ecdsa::Signature {
        secp256k1::ecdsa::Signature::from_compact(BOB_ATTEST_SIGNATURE).unwrap()
    }

    /// Deserializes Dave's [`frost::round2::SignatureShare`].
    pub fn dave_attest_signature_share() -> frost::round2::SignatureShare {
        frost::round2::SignatureShare::deserialize(DAVE_ATTEST_SIGNATURE_SHARE)
            .unwrap()
    }

    /// Deserializes Dave's [`secp256k1::ecdsa::Signature`].
    pub fn dave_attest_signature() -> secp256k1::ecdsa::Signature {
        secp256k1::ecdsa::Signature::from_compact(DAVE_ATTEST_SIGNATURE)
            .unwrap()
    }

    /// Deserializes Eve's [`frost::round2::SignatureShare`].
    pub fn eve_attest_signature_share() -> frost::round2::SignatureShare {
        frost::round2::SignatureShare::deserialize(EVE_ATTEST_SIGNATURE_SHARE)
            .unwrap()
    }

    /// Deserializes Eve's [`secp256k1::ecdsa::Signature`].
    pub fn eve_attest_signature() -> secp256k1::ecdsa::Signature {
        secp256k1::ecdsa::Signature::from_compact(EVE_ATTEST_SIGNATURE).unwrap()
    }

    /// Deserializes Alice's [`secp256k1::SecretKey`].
    pub fn alice_secret_key() -> secp256k1::SecretKey {
        secp256k1::SecretKey::from_slice(ALICE_SECRET_KEY).unwrap()
    }

    /// Deserializes Alice's [`secp256k1::PublicKey`].
    pub fn alice_public_key() -> secp256k1::PublicKey {
        secp256k1::PublicKey::from_slice(ALICE_PUBLIC_KEY).unwrap()
    }

    /// Deserializes Bob's [`secp256k1::PublicKey`].
    pub fn bob_public_key() -> secp256k1::PublicKey {
        secp256k1::PublicKey::from_slice(BOB_PUBLIC_KEY).unwrap()
    }

    /// Deserializes Dave's [`secp256k1::PublicKey`].
    pub fn dave_public_key() -> secp256k1::PublicKey {
        secp256k1::PublicKey::from_slice(DAVE_PUBLIC_KEY).unwrap()
    }

    /// Generates a single-member federation with a random key.
    pub fn fed_members() -> BTreeMap<frost::Identifier, secp256k1::PublicKey> {
        BTreeMap::from([
            (alice_id(), alice_public_key()),
            (bob_id(), bob_public_key()),
            (dave_id(), dave_public_key()),
        ])
    }

    /// Produces a coordinator signature over the sunsetting transcript
    /// commitment for the given multisig ID and public key package.
    // TODO: This should be unified with `btc-server`
    pub fn sign_sunsetting(
        secret_key: &secp256k1::SecretKey,
        multisig_id: MultisigId,
        public_key_package: &PublicKeyPackage,
    ) -> secp256k1::ecdsa::Signature {
        let mut commit = [0; 32];
        let mut t = Transcript::new(b"botanix/multisig-sunsetting/v1");
        t.append_u64(b"multisig_id", multisig_id.as_u32() as u64);
        t.append_message(
            b"public_key_package",
            &public_key_package.serialize().expect("valid serialization"),
        );
        t.challenge_bytes(b"sunsetting_commit", &mut commit);

        let msg =
            secp256k1::Message::from_digest_slice(&commit).expect("valid size");
        let secp = secp256k1::Secp256k1::new();
        secp.sign_ecdsa(&msg, secret_key)
    }

    /// Produces a coordinator signature over the expiration transcript
    /// commitment for the given multisig ID and public key package.
    // TODO: This should be unified with `btc-server`
    pub fn sign_expiration(
        secret_key: &secp256k1::SecretKey,
        multisig_id: MultisigId,
        public_key_package: &PublicKeyPackage,
    ) -> secp256k1::ecdsa::Signature {
        let mut commit = [0; 32];
        let mut t = Transcript::new(b"botanix/multisig-expiration/v1");
        t.append_u64(b"multisig_id", multisig_id.as_u32() as u64);
        t.append_message(
            b"public_key_package",
            &public_key_package.serialize().expect("valid serialization"),
        );
        t.challenge_bytes(b"expiration_commit", &mut commit);

        let msg =
            secp256k1::Message::from_digest_slice(&commit).expect("valid size");
        let secp = secp256k1::Secp256k1::new();
        secp.sign_ecdsa(&msg, secret_key)
    }
}

#[cfg(test)]
mod tests {
    use crate::consensus::multisig_manager::test_data::{
        sign_expiration, sign_sunsetting,
    };

    use super::*;
    use botanix_storage::test_utils::create_test_provider_factory;

    #[test]
    fn callback_try_recv_ready() {
        let (tx, rx) = oneshot::channel::<Result<(), Error>>();
        let callback = SubmissionCallback { rx };

        tx.send(Ok(())).unwrap();

        let result = callback.try_recv();
        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());
    }

    #[test]
    fn callback_try_recv_not_ready() {
        let (tx, rx) = oneshot::channel::<Result<(), Error>>();
        let callback = SubmissionCallback { rx };

        // Returns `Err(self)` when not ready
        let result = callback.try_recv();
        assert!(result.is_err());

        // Retry after getting self back
        let callback = result.unwrap_err();
        tx.send(Ok(())).unwrap();

        let result = callback.try_recv();
        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());
    }

    #[test]
    fn callback_try_recv_error() {
        let (tx, rx) = oneshot::channel::<Result<(), Error>>();
        let callback = SubmissionCallback { rx };

        tx.send(Err(Error::NotAFedMember)).unwrap();

        let result = callback.try_recv();
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Err(Error::NotAFedMember)));
    }

    #[test]
    fn callback_try_recv_shutdown() {
        let (tx, rx) = oneshot::channel::<Result<(), Error>>();
        let callback = SubmissionCallback { rx };

        // Drop sender without sending, implies shutdown
        drop(tx);

        let result = callback.try_recv();
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Err(Error::Shutdown)));
    }

    #[test]
    fn callback_blocking_recv() {
        let (tx, rx) = oneshot::channel::<Result<(), Error>>();
        let callback = SubmissionCallback { rx };

        tx.send(Ok(())).unwrap();

        let result = callback.blocking_recv();
        assert!(result.is_ok());
    }

    #[test]
    fn callback_blocking_recv_error() {
        let (tx, rx) = oneshot::channel::<Result<(), Error>>();
        let callback = SubmissionCallback { rx };

        tx.send(Err(Error::NotAFedMember)).unwrap();

        let result = callback.blocking_recv();
        assert!(matches!(result, Err(Error::NotAFedMember)));
    }

    #[test]
    fn callback_blocking_recv_shutdown() {
        let (tx, rx) = oneshot::channel::<Result<(), Error>>();
        let callback = SubmissionCallback { rx };

        // Drop sender without sending, implies shutdown
        drop(tx);

        let result = callback.blocking_recv();
        assert!(matches!(result, Err(Error::Shutdown)));
    }

    #[tokio::test]
    async fn callback_async() {
        let (tx, rx) = oneshot::channel::<Result<(), Error>>();
        let callback = SubmissionCallback { rx };

        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            tx.send(Ok(())).unwrap();
        });

        let result = callback.await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn callback_async_error() {
        let (tx, rx) = oneshot::channel::<Result<(), Error>>();
        let callback = SubmissionCallback { rx };

        tx.send(Err(Error::NotAFedMember)).unwrap();

        let result = callback.await;
        assert!(matches!(result, Err(Error::NotAFedMember)));
    }

    #[tokio::test]
    async fn callback_async_shutdown() {
        let (tx, rx) = oneshot::channel::<Result<(), Error>>();
        let callback = SubmissionCallback { rx };

        // Drop sender without sending, implies shutdown
        drop(tx);

        let result = callback.await;
        assert!(matches!(result, Err(Error::Shutdown)));
    }

    #[test]
    fn test_submitter_shutdown_immediate_error() {
        let factory = create_test_provider_factory();
        let (manager, submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        // Drop manager to simulate shutdown
        drop(manager);

        // Submit should still return a callback
        let callback = submitter.submit_expiration(
            MultisigId::new(1),
            secp256k1::ecdsa::Signature::from_compact(&[1u8; 64]).unwrap(),
        );

        // Callback should immediately resolve to shutdown error
        let result = callback.try_recv();
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Err(Error::Shutdown)));
    }

    #[test]
    fn insert_and_retrieve_staging_multisig() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        let id = MultisigId::new(99);

        // Nothing inserted, should be None.
        let found = manager.client.get_staging_multisig(id).unwrap();
        assert!(found.is_none());

        let entry = StagedMultisigEntry {
            multisig_id: id,
            coordinator: test_data::alice_id(),
            fed_members: test_data::fed_members(),
        };

        manager.client.insert_staging_multisig(id, entry).unwrap();

        // Data should be retrievable.
        let found = manager.client.get_staging_multisig(id).unwrap();
        assert!(found.is_some());
    }

    #[test]
    fn guard_commit_persists_data() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        let id = MultisigId::new(99);
        let entry = StagedMultisigEntry {
            multisig_id: id,
            coordinator: test_data::alice_id(),
            fed_members: test_data::fed_members(),
        };

        manager.client.insert_staging_multisig(id, entry).unwrap();

        // Data should be visible after insert.
        let found = manager.client.get_staging_multisig(id).unwrap();
        assert!(found.is_some());
    }

    #[test]
    fn legacy_preload_is_funding_and_active() {
        let legacy = test_data::preload_legacy();

        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(legacy.clone())).unwrap();

        // The legacy preload should be the sole funding multisig and the
        // only entry in the active set.
        let funding = manager.get_funding_multisig().unwrap().unwrap();
        let active = manager.get_active_multisigs().unwrap();

        assert_eq!(funding, legacy);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0], legacy);
    }

    /// Attesting a multisig that was never staged fails.
    #[test]
    fn attestation_rejects_unstaged_multisig() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        // No staging entry exists for UPGRADE_MULTISIG.
        let result = manager.recv(test_data::attestation_message());
        assert!(matches!(result, Err(Error::LifecycleMustBeStaged)));
    }

    /// Attesting a multisig that is already funding (not staged) fails. The
    /// legacy multisig starts as funding -- attesting it should be rejected
    /// because there is no staging entry for it.
    #[test]
    fn attestation_rejects_funding_multisig() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        // Patch the attestation to target the legacy (funding) multisig.
        let mut msg = test_data::attestation_message();
        let Message::Attestation { multisig_id, .. } = &mut msg else {
            unreachable!()
        };
        *multisig_id = test_data::LEGACY_MULTISIG;

        let result = manager.recv(msg);
        assert!(matches!(result, Err(Error::LifecycleMustBeStaged)));
    }

    /// Attesting a sunsetting multisig fails -- only staged multisigs can be
    /// attested. Drive the legacy multisig through the full lifecycle to reach
    /// the sunsetting state, then attempt attestation.
    #[test]
    fn attestation_rejects_sunsetting_multisig() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        let legacy_id = test_data::LEGACY_MULTISIG;
        let alice_sec = test_data::alice_secret_key();
        let pkp = test_data::public_key_package();

        // Stage + attest upgrade -> legacy becomes degrading.
        manager
            .set_staging_multisig(
                test_data::UPGRADE_MULTISIG,
                test_data::alice_id(),
                test_data::fed_members(),
            )
            .unwrap();

        manager.recv(test_data::attestation_message()).unwrap();

        // Sunset the degrading legacy -> legacy becomes sunsetting.
        let msg = Message::Sunsetting {
            multisig_id: legacy_id,
            coordinator_signature: sign_sunsetting(&alice_sec, legacy_id, &pkp),
        };

        manager.recv(msg).unwrap();

        // Attempt to attest the now-sunsetting legacy multisig.
        let mut msg = test_data::attestation_message();
        let Message::Attestation { multisig_id, .. } = &mut msg else {
            unreachable!()
        };
        *multisig_id = legacy_id;

        let result = manager.recv(msg);
        assert!(matches!(result, Err(Error::LifecycleMustBeStaged)));
    }

    /// Attestation with a mismatched multisig ID fails signature verification
    /// because the signing transcript commits to the ID.
    #[test]
    fn attestation_rejects_wrong_multisig_id() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        // Stage under a different ID than the one the signatures commit to.
        let wrong_id = MultisigId::new(1);
        assert_ne!(wrong_id, test_data::UPGRADE_MULTISIG);

        manager
            .set_staging_multisig(
                wrong_id,
                test_data::alice_id(),
                test_data::fed_members(),
            )
            .unwrap();

        // Patch the attestation to claim the wrong ID.
        let mut msg = test_data::attestation_message();
        let Message::Attestation { multisig_id, .. } = &mut msg else {
            unreachable!()
        };
        *multisig_id = wrong_id;

        let err = manager.recv(msg).unwrap_err();
        assert!(matches!(err, Error::SignatureVerificationFailed));
    }

    /// Swapping a single FROST signature share causes aggregation to fail.
    #[test]
    fn attestation_rejects_wrong_signature_share() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        manager
            .set_staging_multisig(
                test_data::UPGRADE_MULTISIG,
                test_data::alice_id(),
                test_data::fed_members(),
            )
            .unwrap();

        // Replace Dave's share with Eve's (wrong share, correct slot).
        let mut msg = test_data::attestation_message();
        let Message::Attestation { signatures, .. } = &mut msg else {
            unreachable!()
        };
        let (share, _) = signatures.get_mut(&test_data::dave_id()).unwrap();
        *share = test_data::eve_attest_signature_share();

        let err = manager.recv(msg).unwrap_err();
        assert!(matches!(err, Error::SignatureVerificationFailed));
    }

    /// Swapping a single secp256k1 attestation signature causes verification to
    /// fail.
    #[test]
    fn attestation_rejects_wrong_attest_signature() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        manager
            .set_staging_multisig(
                test_data::UPGRADE_MULTISIG,
                test_data::alice_id(),
                test_data::fed_members(),
            )
            .unwrap();

        // Replace Dave's attestation signature with Eve's.
        let mut msg = test_data::attestation_message();
        let Message::Attestation { signatures, .. } = &mut msg else {
            unreachable!()
        };
        let (_, sig) = signatures.get_mut(&test_data::dave_id()).unwrap();
        *sig = test_data::eve_attest_signature();

        let err = manager.recv(msg).unwrap_err();
        assert!(matches!(err, Error::SignatureVerificationFailed));
    }

    /// Stages a new multisig, attests it with valid DKG signatures, and
    /// verifies the lifecycle transition: the new multisig becomes the funding
    /// multisig while the legacy one is demoted to degrading.
    #[test]
    fn attestation_promotes_staged_to_funding() {
        let legacy = test_data::preload_legacy();

        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(legacy.clone())).unwrap();

        // Only the legacy multisig is active and funding.
        assert_eq!(
            manager.get_active_multisigs().unwrap(),
            vec![legacy.clone()]
        );
        assert_eq!(manager.get_funding_multisig().unwrap().unwrap(), legacy);

        let multisig_id = test_data::UPGRADE_MULTISIG;
        let coordinator = test_data::alice_id();
        let fed_members = test_data::fed_members();
        assert!(fed_members.contains_key(&coordinator));

        // Stage a new multisig for upgrade.
        manager
            .set_staging_multisig(multisig_id, coordinator, fed_members)
            .unwrap();

        // Staging does not change the active or funding set.
        assert_eq!(
            manager.get_active_multisigs().unwrap(),
            vec![legacy.clone()]
        );
        assert_eq!(manager.get_funding_multisig().unwrap().unwrap(), legacy);

        // Attest the staged multisig with valid DKG signatures.
        manager.recv(test_data::attestation_message()).unwrap();

        // Both multisigs are now active (legacy degraded, new one funding).
        let active = manager.get_active_multisigs().unwrap();
        assert_eq!(active.len(), 2);

        // The new multisig is now the funding multisig.
        let funding = manager.get_funding_multisig().unwrap().unwrap();
        assert_eq!(funding.multisig_id, test_data::UPGRADE_MULTISIG);
    }

    /// Sunsetting a multisig that does not exist fails.
    #[test]
    fn sunsetting_rejects_unknown_multisig() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        let unknown_id = MultisigId::new(42);
        let msg = Message::Sunsetting {
            multisig_id: unknown_id,
            coordinator_signature: sign_sunsetting(
                &test_data::alice_secret_key(),
                unknown_id,
                &test_data::public_key_package(),
            ),
        };

        let result = manager.recv(msg);
        assert!(matches!(result, Err(Error::LifecycleMustBeDegraded)));
    }

    /// Sunsetting a funding multisig fails -- only degrading multisigs
    /// can be sunset.
    #[test]
    fn sunsetting_rejects_funding_multisig() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        // The legacy multisig is funding (not degrading).
        let legacy_id = test_data::LEGACY_MULTISIG;
        let msg = Message::Sunsetting {
            multisig_id: legacy_id,
            coordinator_signature: sign_sunsetting(
                &test_data::alice_secret_key(),
                legacy_id,
                &test_data::public_key_package(),
            ),
        };

        let result = manager.recv(msg);
        assert!(matches!(result, Err(Error::LifecycleMustBeDegraded)));
    }

    /// Expiring a multisig that does not exist fails.
    #[test]
    fn expiration_rejects_unknown_multisig() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        let unknown_id = MultisigId::new(42);
        let msg = Message::Expiration {
            multisig_id: unknown_id,
            coordinator_signature: sign_expiration(
                &test_data::alice_secret_key(),
                unknown_id,
                &test_data::public_key_package(),
            ),
        };

        let result = manager.recv(msg);
        assert!(matches!(result, Err(Error::LifecycleMustBeSunset)));
    }

    /// Expiring a staged multisig fails -- only sunsetting multisigs can be
    /// expired.
    #[test]
    fn expiration_rejects_staged_multisig() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        let upgrade_id = test_data::UPGRADE_MULTISIG;

        // Stage a multisig but don't attest it.
        manager
            .set_staging_multisig(
                upgrade_id,
                test_data::alice_id(),
                test_data::fed_members(),
            )
            .unwrap();

        let msg = Message::Expiration {
            multisig_id: upgrade_id,
            coordinator_signature: sign_expiration(
                &test_data::alice_secret_key(),
                upgrade_id,
                &test_data::public_key_package(),
            ),
        };

        let result = manager.recv(msg);
        assert!(matches!(result, Err(Error::LifecycleMustBeSunset)));
    }

    /// Expiring a funding multisig fails -- it must be sunsetting first.
    #[test]
    fn expiration_rejects_funding_multisig() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        // The legacy multisig is funding.
        let legacy_id = test_data::LEGACY_MULTISIG;
        let msg = Message::Expiration {
            multisig_id: legacy_id,
            coordinator_signature: sign_expiration(
                &test_data::alice_secret_key(),
                legacy_id,
                &test_data::public_key_package(),
            ),
        };

        let result = manager.recv(msg);
        assert!(matches!(result, Err(Error::LifecycleMustBeSunset)));
    }

    /// Expiring a degrading multisig fails -- it must be sunset first.
    #[test]
    fn expiration_rejects_degrading_multisig() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        // Stage + attest upgrade -> legacy becomes degrading.
        manager
            .set_staging_multisig(
                test_data::UPGRADE_MULTISIG,
                test_data::alice_id(),
                test_data::fed_members(),
            )
            .unwrap();
        manager.recv(test_data::attestation_message()).unwrap();

        // Attempt to expire the degrading legacy multisig directly.
        let legacy_id = test_data::LEGACY_MULTISIG;
        let msg = Message::Expiration {
            multisig_id: legacy_id,
            coordinator_signature: sign_expiration(
                &test_data::alice_secret_key(),
                legacy_id,
                &test_data::public_key_package(),
            ),
        };

        let result = manager.recv(msg);
        assert!(matches!(result, Err(Error::LifecycleMustBeSunset)));
    }

    /// Full lifecycle: stage -> attest -> sunset -> expire.
    ///
    /// After attestation promotes the upgrade multisig to funding, the legacy
    /// multisig is demoted to degrading. The coordinator then sunsets it
    /// (removing it from the active set) and finally expires it (deleting it
    /// entirely).
    #[test]
    fn full_lifecycle_stage_attest_sunset_expire() {
        let factory = create_test_provider_factory();
        let (manager, _submitter) =
            MultisigManager::new(factory, Some(test_data::preload_legacy()))
                .unwrap();

        let legacy_id = test_data::LEGACY_MULTISIG;
        let upgrade_id = test_data::UPGRADE_MULTISIG;
        let alice_sec = test_data::alice_secret_key();
        let pkp = test_data::public_key_package();

        // Stage and attest the upgrade multisig.
        manager
            .set_staging_multisig(
                upgrade_id,
                test_data::alice_id(),
                test_data::fed_members(),
            )
            .unwrap();
        manager.recv(test_data::attestation_message()).unwrap();

        // Legacy is now degrading, upgrade is funding -- both active.
        assert_eq!(manager.get_active_multisigs().unwrap().len(), 2);
        assert_eq!(
            manager.get_funding_multisig().unwrap().unwrap().multisig_id,
            upgrade_id
        );

        // Sunset the degrading legacy multisig.
        let msg = Message::Sunsetting {
            multisig_id: legacy_id,
            coordinator_signature: sign_sunsetting(&alice_sec, legacy_id, &pkp),
        };
        manager.recv(msg).unwrap();

        // Only the upgrade multisig remains active.
        assert_eq!(manager.get_active_multisigs().unwrap().len(), 1);
        assert_eq!(
            manager.get_funding_multisig().unwrap().unwrap().multisig_id,
            upgrade_id
        );

        // Expire the sunset legacy multisig.
        let msg = Message::Expiration {
            multisig_id: legacy_id,
            coordinator_signature: sign_expiration(&alice_sec, legacy_id, &pkp),
        };
        manager.recv(msg).unwrap();
    }
}
