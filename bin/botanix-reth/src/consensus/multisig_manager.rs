use botanix_types::MultisigId;
use frost_secp256k1_tr::{self as frost, keys::PublicKeyPackage};
use merlin::Transcript;
use std::{
    collections::BTreeMap,
    sync::{mpsc, Arc, Mutex, MutexGuard},
};
use tokio::sync::oneshot;

/// Errors from multisig lifecycle operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Participant is not a federation member.
    #[error("Not a member of the federation")]
    NotAFedMember,
    /// Signature verification failed.
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
    /// Missing signature shares for n-of-n aggregation.
    #[error("Not enough signature shares available")]
    NotEnoughSignatureShares,
    /// Internal FROST library error.
    #[allow(clippy::enum_variant_names)]
    #[error("Unexpected internal Frost error")]
    InternalFrostError(#[from] frost::Error),
    /// Multisig ID not found in manager.
    #[error("Multisig ID does not exist")]
    MultisigIdNotExist,
    /// Attestation requires Staged lifecycle state.
    #[error("Lifecycle must be Staged for attestation")]
    LifecycleMustBeStaged,
    /// Expiration requires Sunset lifecycle state.
    #[error("Lifecycle must be Sunset for expiration")]
    LifecycleMustBeSunset,
    /// Manager has shut down.
    #[error("Manager has shut down")]
    Shutdown,
}

struct ChannelPayload {
    message: Message,
    callback: oneshot::Sender<Result<(), Error>>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Expiration {
        multisig_id: MultisigId,
        coordinator_signature: secp256k1::ecdsa::Signature,
    },
    Attestation {
        multisig_id: MultisigId,
        public_key_package: PublicKeyPackage,
        signing_package: frost::SigningPackage,
        signatures: BTreeMap<
            frost::Identifier,
            (frost::round2::SignatureShare, secp256k1::ecdsa::Signature),
        >,
    },
}

/// Serialization error for [`Message`].
#[derive(Debug, thiserror::Error)]
pub enum SerializationError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("FROST serialization error: {0}")]
    Frost(#[from] frost::Error),
    #[error("secp256k1 error: {0}")]
    Secp256k1(#[from] secp256k1::Error),
    #[error("unknown message variant: {0}")]
    UnknownVariant(u8),
}

/// Message variant tags.
const MESSAGE_VARIANT_EXPIRATION: u8 = 0x00;
const MESSAGE_VARIANT_ATTESTATION: u8 = 0x01;

impl Message {
    /// Serializes the message to bytes.
    // TODO: Consider just using ciborium for this?
    pub fn serialize(&self) -> Result<Vec<u8>, SerializationError> {
        use std::io::Write;

        let mut writer = Vec::new();

        match self {
            Message::Expiration { multisig_id, coordinator_signature } => {
                writer.write_all(&[MESSAGE_VARIANT_EXPIRATION])?;
                writer.write_all(&multisig_id.as_u32().to_le_bytes())?;
                writer.write_all(&coordinator_signature.serialize_compact())?;
            }
            Message::Attestation {
                multisig_id,
                public_key_package,
                signing_package,
                signatures,
            } => {
                writer.write_all(&[MESSAGE_VARIANT_ATTESTATION])?;
                writer.write_all(&multisig_id.as_u32().to_le_bytes())?;

                // Length-prefixed public key package.
                let pkp_bytes = public_key_package.serialize()?;
                writer.write_all(&(pkp_bytes.len() as u32).to_le_bytes())?;
                writer.write_all(&pkp_bytes)?;

                // Length-prefixed signing package.
                let sp_bytes = signing_package.serialize()?;
                writer.write_all(&(sp_bytes.len() as u32).to_le_bytes())?;
                writer.write_all(&sp_bytes)?;

                // Signatures map: count followed by entries.
                writer.write_all(&(signatures.len() as u32).to_le_bytes())?;
                for (frost_id, (sig_share, att_sig)) in signatures {
                    writer.write_all(&frost_id.serialize())?;
                    writer.write_all(&sig_share.serialize())?;
                    writer.write_all(&att_sig.serialize_compact())?;
                }
            }
        }

        Ok(writer)
    }

    /// Deserializes a message from a reader.
    // TODO: Consider just using ciborium for this?
    pub fn deserialize(
        reader: &mut impl std::io::Read,
    ) -> Result<Self, SerializationError> {
        let mut variant = [0u8; 1];
        reader.read_exact(&mut variant)?;

        match variant[0] {
            MESSAGE_VARIANT_EXPIRATION => {
                let mut multisig_id_bytes = [0u8; 4];
                reader.read_exact(&mut multisig_id_bytes)?;
                let multisig_id =
                    MultisigId::new(u32::from_le_bytes(multisig_id_bytes));

                let mut sig_bytes = [0u8; 64];
                reader.read_exact(&mut sig_bytes)?;
                let coordinator_signature =
                    secp256k1::ecdsa::Signature::from_compact(&sig_bytes)?;

                Ok(Message::Expiration { multisig_id, coordinator_signature })
            }
            MESSAGE_VARIANT_ATTESTATION => {
                let mut multisig_id_bytes = [0u8; 4];
                reader.read_exact(&mut multisig_id_bytes)?;
                let multisig_id =
                    MultisigId::new(u32::from_le_bytes(multisig_id_bytes));

                // Read length-prefixed public key package.
                let mut len_bytes = [0u8; 4];
                reader.read_exact(&mut len_bytes)?;
                let pkp_len = u32::from_le_bytes(len_bytes) as usize;
                let mut pkp_bytes = vec![0u8; pkp_len];
                reader.read_exact(&mut pkp_bytes)?;
                let public_key_package =
                    PublicKeyPackage::deserialize(&pkp_bytes)?;

                // Read length-prefixed signing package.
                reader.read_exact(&mut len_bytes)?;
                let sp_len = u32::from_le_bytes(len_bytes) as usize;
                let mut sp_bytes = vec![0u8; sp_len];
                reader.read_exact(&mut sp_bytes)?;
                let signing_package =
                    frost::SigningPackage::deserialize(&sp_bytes)?;

                // Read signatures map.
                reader.read_exact(&mut len_bytes)?;
                let sig_count = u32::from_le_bytes(len_bytes) as usize;
                let mut signatures = BTreeMap::new();

                for _ in 0..sig_count {
                    let mut frost_id_bytes = [0u8; 32];
                    reader.read_exact(&mut frost_id_bytes)?;
                    let frost_id =
                        frost::Identifier::deserialize(&frost_id_bytes)?;

                    let mut sig_share_bytes = [0u8; 32];
                    reader.read_exact(&mut sig_share_bytes)?;
                    let sig_share = frost::round2::SignatureShare::deserialize(
                        &sig_share_bytes,
                    )?;

                    let mut att_sig_bytes = [0u8; 64];
                    reader.read_exact(&mut att_sig_bytes)?;
                    let att_sig = secp256k1::ecdsa::Signature::from_compact(
                        &att_sig_bytes,
                    )?;

                    signatures.insert(frost_id, (sig_share, att_sig));
                }

                Ok(Message::Attestation {
                    multisig_id,
                    public_key_package,
                    signing_package,
                    signatures,
                })
            }
            v => Err(SerializationError::UnknownVariant(v)),
        }
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
/// Supports submitting attestations (to activate staged multisigs) and
/// expirations (to remove sunset multisigs). Messages are validated before
/// being proposed to consensus.
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

/// Lifecycle state of a multisig within the federation.
///
/// Multisigs progress through these states:
/// 1. **Staged**: Newly created, awaiting async DKG setup with resulting
///    attestation from all federation members.
/// 2. **Active**: Fully attested and available for signing operations.
/// 3. **Sunset**: Deprecated and awaiting eventual expiration by the
///    coordinator.
///
/// After expiration, the multisig is removed entirely from the manager.
#[derive(Debug, Clone)]
enum Lifecycle {
    /// Awaiting attestation from all federation members before activation.
    Staged {
        coordinator: frost::Identifier,
        fed_members: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
    },
    /// Fully attested and available for signing operations.
    Active {
        coordinator: frost::Identifier,
        fed_members: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
        public_key_package: PublicKeyPackage,
    },
    /// Deprecated and awaiting expiration by the coordinator.
    Sunset {
        coordinator: frost::Identifier,
        fed_members: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
        public_key_package: PublicKeyPackage,
    },
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
/// manager.set_staged(multisig_id, coordinator, fed_members);
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
pub struct MultisigManager {
    // TODO: Consider using a sync/bounded channel?
    queue: Arc<Mutex<mpsc::Receiver<ChannelPayload>>>,
    submitter: MultisigSubmitter,
    multisigs: Arc<Mutex<BTreeMap<MultisigId, Lifecycle>>>,
}

// TODO: Those `set_*` methods should only be available during building.
impl MultisigManager {
    /// Creates a new manager and its associated submitter handle.
    pub fn new() -> (Self, MultisigSubmitter) {
        let (tx, rx) = mpsc::channel();

        let submitter = MultisigSubmitter { queue: tx };
        let this = MultisigManager {
            queue: Arc::new(Mutex::new(rx)),
            submitter: submitter.clone(),
            multisigs: Default::default(),
        };

        (this, submitter)
    }
    /// Returns a cloned handle to the [`MultisigSubmitter`] for this manager.
    pub fn submitter(&self) -> MultisigSubmitter {
        self.submitter.clone()
    }
    /// Registers a new multisig in the _Staged_ state.
    ///
    /// A staged multisig is awaiting attestation from all federation members
    /// before it can become active.
    pub fn set_staged(
        &mut self,
        multisig_id: MultisigId,
        coordinator: frost::Identifier,
        fed_members: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
    ) {
        let mut l = self.multisigs.lock().expect("poisoned lock");
        l.insert(multisig_id, Lifecycle::Staged { coordinator, fed_members });
    }
    /// Transitions a multisig to the _Active_ state.
    ///
    /// An active multisig has been attested by all federation members and can
    /// participate in signing operations.
    pub fn set_active(
        &mut self,
        multisig_id: MultisigId,
        coordinator: frost::Identifier,
        fed_members: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
        public_key_package: PublicKeyPackage,
    ) {
        let mut l = self.multisigs.lock().expect("poisoned lock");
        l.insert(
            multisig_id,
            Lifecycle::Active { coordinator, fed_members, public_key_package },
        );
    }
    /// Transitions a multisig to the Sunset state.
    ///
    /// A sunset multisig is no longer actively used for signing but awaits
    /// expiration by the coordinator before being fully removed.
    pub fn set_sunset(
        &mut self,
        multisig_id: MultisigId,
        coordinator: frost::Identifier,
        fed_members: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
        public_key_package: PublicKeyPackage,
    ) {
        let mut l = self.multisigs.lock().expect("poisoned lock");
        l.insert(
            multisig_id,
            Lifecycle::Sunset { coordinator, fed_members, public_key_package },
        );
    }
    /// Returns all multisigs currently in the _Active_ state.
    ///
    /// Each entry contains the multisig ID and its associated public key package.
    pub fn get_active(&self) -> Vec<(MultisigId, PublicKeyPackage)> {
        let l = self.multisigs.lock().expect("poisoned lock");
        l.iter()
            .filter_map(|(id, lifecycle)| match lifecycle {
                Lifecycle::Active { public_key_package, .. } => {
                    Some((*id, public_key_package.clone()))
                }
                _ => None,
            })
            .collect()
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
        // accidently propose invalid messages to the conensus layer.
        let mut l = self.multisigs.lock().expect("poisoned lock");
        let mut validate_messages = || match payload.message.clone() {
            Message::Attestation {
                multisig_id,
                public_key_package,
                signing_package,
                signatures,
            } => Self::validate_attestation_dry_run(
                multisig_id,
                public_key_package,
                signing_package,
                signatures,
                &mut l,
            ),
            Message::Expiration {
                multisig_id, //
                coordinator_signature,
            } => Self::validate_expiration_dry_run(
                &multisig_id,
                &coordinator_signature,
                &mut l,
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
        let mut l = self.multisigs.lock().expect("poisoned lock");

        match msg {
            Message::Attestation {
                multisig_id,
                public_key_package,
                signing_package,
                signatures,
            } => Self::validate_attestation(
                multisig_id,
                public_key_package,
                signing_package,
                signatures,
                &mut l,
            ),
            Message::Expiration {
                multisig_id, //
                coordinator_signature,
            } => Self::validate_expiration(
                &multisig_id, //
                &coordinator_signature,
                &mut l,
            ),
        }
    }
    /// Validates a FROST attestation without modifying state.
    ///
    /// Verifies that all federation members correctly participated in the DKG
    /// ceremony by checking each member's FROST signature share and attestation
    /// signature, then aggregating into the final group signature.
    fn validate_attestation_dry_run(
        multisig_id: MultisigId,
        public_key_package: PublicKeyPackage,
        signing_package: frost::SigningPackage,
        signatures: BTreeMap<
            frost::Identifier,
            (frost::round2::SignatureShare, secp256k1::ecdsa::Signature),
        >,
        multisigs: &mut MutexGuard<'_, BTreeMap<MultisigId, Lifecycle>>,
    ) -> Result<(), Error> {
        // Only Staged multisigs can be attested.
        let Lifecycle::Staged { coordinator, fed_members } = multisigs
            .get(&multisig_id)
            .cloned()
            .ok_or(Error::MultisigIdNotExist)?
        else {
            return Err(Error::LifecycleMustBeStaged);
        };

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
        multisig_id: MultisigId,
        public_key_package: PublicKeyPackage,
        signing_package: frost::SigningPackage,
        signatures: BTreeMap<
            frost::Identifier,
            (frost::round2::SignatureShare, secp256k1::ecdsa::Signature),
        >,
        multisigs: &mut MutexGuard<'_, BTreeMap<MultisigId, Lifecycle>>,
    ) -> Result<(), Error> {
        Self::validate_attestation_dry_run(
            multisig_id,
            public_key_package.clone(),
            signing_package,
            signatures,
            multisigs,
        )?;

        // Transition the multisig from Staged to Active.
        let Lifecycle::Staged { coordinator, fed_members } =
            multisigs.remove(&multisig_id).unwrap()
        else {
            unreachable!("dry-run verified lifecycle is Staged")
        };

        // Sunset any currently Active multisigs before activating the new one.
        //
        // TODO: Consider simplifying the `self.multisigs` structure by
        // mandating that there can only be one staged, one active and multiple
        // sunset multisigs--instead of keeping it all in one single list.
        let active_ids: Vec<_> = multisigs
            .iter()
            .filter(|(id, lc)| {
                **id != multisig_id && matches!(lc, Lifecycle::Active { .. })
            })
            .map(|(id, _)| *id)
            .collect();

        for id in active_ids {
            let Lifecycle::Active {
                coordinator,
                fed_members,
                public_key_package,
            } = multisigs.remove(&id).unwrap()
            else {
                unreachable!("lifecycles filtered beforehand")
            };

            multisigs.insert(
                id,
                Lifecycle::Sunset {
                    coordinator,
                    fed_members,
                    public_key_package,
                },
            );
        }

        // Set new multisig as Active.
        multisigs.insert(
            multisig_id,
            Lifecycle::Active { coordinator, public_key_package, fed_members },
        );

        Ok(())
    }
    /// Validates an expiration request without modifying state.
    ///
    /// Verifies the coordinator's signature over a commitment binding the
    /// multisig ID and public key package.
    fn validate_expiration_dry_run(
        multisig_id: &MultisigId,
        coordinator_signature: &secp256k1::ecdsa::Signature,
        multisigs: &mut MutexGuard<'_, BTreeMap<MultisigId, Lifecycle>>,
    ) -> Result<(), Error> {
        // Only Sunset multisigs can be expired.
        let Lifecycle::Sunset { coordinator, fed_members, public_key_package } =
            multisigs.get(multisig_id).ok_or(Error::MultisigIdNotExist)?
        else {
            return Err(Error::LifecycleMustBeSunset);
        };

        let coord_pubkey = fed_members
            .get(&coordinator)
            .expect("coordinator pubkey must exist");

        // Build transcript commitment over the multisig identity.
        let mut commit = [0; 32];

        {
            let mut t = Transcript::new(b"botanix/multisig-expiration/v1");
            t.append_u64(
                b"multisig_id", //
                multisig_id.as_u32() as u64,
            );
            t.append_message(
                b"public_key_package",
                public_key_package.serialize()?.as_slice(),
            );
            t.challenge_bytes(b"expiration_commit", &mut commit);
        }

        // Verify the coordinator's signature over the commitment.
        let msg =
            secp256k1::Message::from_digest_slice(&commit).expect("valid size");

        let secp = secp256k1::Secp256k1::new();
        coord_pubkey
            .verify(&secp, &msg, &coordinator_signature)
            .map_err(|_| Error::SignatureVerificationFailed)?;

        Ok(())
    }
    /// Validates an expiration request and removes the multisig.
    ///
    /// Finalizes the sunset process by verifying the coordinator's signature,
    /// then removing the multisig entirely. Only the designated coordinator can
    /// authorize expiration, and only for multisigs in the Sunset state.
    fn validate_expiration(
        multisig_id: &MultisigId,
        coordinator_signature: &secp256k1::ecdsa::Signature,
        multisigs: &mut MutexGuard<'_, BTreeMap<MultisigId, Lifecycle>>,
    ) -> Result<(), Error> {
        Self::validate_expiration_dry_run(
            multisig_id,
            coordinator_signature,
            multisigs,
        )?;

        // Safe to unwrap: dry_run verified the multisig exists.
        let prev = multisigs.remove(multisig_id);
        debug_assert!(prev.is_some());

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
        t.append_message(
            b"signing_package",
            signing_package.serialize()?.as_slice(),
        );
        t.append_message(
            b"public_key_package",
            public_key_package.serialize()?.as_slice(),
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
            let mut t = self.transcript.clone();
            t.append_message(
                b"signature_share",
                signature_share.serialize().as_slice(),
            );
            t.challenge_bytes(b"attestation_commit", &mut commit);
        }

        let msg =
            secp256k1::Message::from_digest_slice(&commit).expect("valid size");
        fed.verify(&self.secp, &msg, &attestation_sig)
            .map_err(|_| Error::SignatureVerificationFailed)?;

        self.signature_shares.insert(id, signature_share);

        Ok(())
    }
    fn try_finalize(&mut self) -> Result<Option<frost::Signature>, Error> {
        if self.signature_shares.len() != self.fed_members.len() {
            return Ok(None);
        }

        self.finalize().map(Some)
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

#[cfg(test)]
mod tests {
    use super::*;

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

        tx.send(Err(Error::MultisigIdNotExist)).unwrap();

        let result = callback.try_recv();
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Err(Error::MultisigIdNotExist)));
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

        tx.send(Err(Error::MultisigIdNotExist)).unwrap();

        let result = callback.blocking_recv();
        assert!(matches!(result, Err(Error::MultisigIdNotExist)));
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

        tx.send(Err(Error::MultisigIdNotExist)).unwrap();

        let result = callback.await;
        assert!(matches!(result, Err(Error::MultisigIdNotExist)));
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
        let (manager, submitter) = MultisigManager::new();

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

    // --- Lifecycle state tests ---

    mod test_data {
        use super::*;

        const SIGNING_PACKAGE: &[u8] = &[
            0, 35, 15, 138, 179, 3, 12, 223, 219, 150, 131, 74, 116, 233, 236,
            196, 96, 205, 96, 130, 121, 192, 88, 21, 198, 81, 210, 88, 18, 92,
            42, 123, 15, 91, 242, 12, 143, 253, 0, 35, 15, 138, 179, 3, 43,
            183, 127, 113, 151, 32, 250, 130, 254, 91, 25, 188, 8, 212, 187,
            35, 237, 165, 216, 46, 22, 216, 81, 160, 215, 226, 89, 7, 79, 72,
            14, 128, 2, 87, 196, 246, 142, 137, 19, 46, 149, 78, 198, 191, 65,
            185, 253, 136, 195, 141, 249, 51, 23, 96, 28, 44, 131, 66, 9, 167,
            165, 44, 90, 191, 218, 52, 39, 160, 118, 176, 185, 222, 25, 54,
            213, 2, 171, 233, 2, 75, 75, 154, 59, 199, 10, 16, 208, 24, 249,
            238, 56, 171, 146, 37, 245, 114, 93, 0, 35, 15, 138, 179, 3, 207,
            251, 59, 134, 95, 94, 103, 1, 172, 126, 225, 224, 96, 127, 10, 185,
            236, 128, 32, 40, 209, 198, 90, 200, 220, 31, 63, 18, 177, 216, 57,
            101, 2, 106, 175, 117, 226, 120, 173, 75, 59, 129, 208, 97, 68,
            205, 52, 98, 217, 55, 51, 251, 246, 211, 132, 28, 121, 108, 180,
            32, 108, 149, 136, 98, 61, 172, 197, 159, 249, 40, 76, 205, 49,
            208, 14, 123, 169, 145, 252, 96, 128, 142, 96, 26, 2, 128, 79, 6,
            59, 90, 29, 133, 161, 26, 217, 244, 230, 0, 35, 15, 138, 179, 3,
            64, 219, 163, 189, 191, 13, 1, 215, 112, 149, 123, 158, 147, 1,
            209, 222, 223, 201, 31, 118, 91, 112, 250, 176, 83, 226, 246, 50,
            192, 125, 106, 135, 2, 185, 73, 250, 61, 245, 26, 151, 100, 182,
            227, 54, 73, 24, 248, 12, 248, 88, 35, 107, 253, 77, 144, 74, 152,
            191, 236, 147, 251, 59, 195, 44, 249, 32, 22, 196, 180, 100, 72,
            245, 46, 75, 131, 238, 89, 67, 214, 113, 87, 250, 53, 60, 122, 26,
            179, 42, 161, 188, 88, 140, 8, 155, 227, 122, 147, 179,
        ];

        const PUBLIC_KEY_PACKAGE: &[u8] = &[
            0, 35, 15, 138, 179, 3, 12, 223, 219, 150, 131, 74, 116, 233, 236,
            196, 96, 205, 96, 130, 121, 192, 88, 21, 198, 81, 210, 88, 18, 92,
            42, 123, 15, 91, 242, 12, 143, 253, 3, 19, 150, 56, 171, 82, 61,
            62, 154, 169, 9, 64, 4, 237, 106, 19, 60, 180, 209, 122, 148, 209,
            35, 117, 76, 166, 249, 95, 205, 239, 111, 220, 36, 52, 39, 160,
            118, 176, 185, 222, 25, 54, 213, 2, 171, 233, 2, 75, 75, 154, 59,
            199, 10, 16, 208, 24, 249, 238, 56, 171, 146, 37, 245, 114, 93, 3,
            21, 241, 79, 252, 160, 143, 247, 68, 21, 240, 134, 27, 206, 127,
            173, 62, 94, 60, 181, 190, 186, 138, 7, 176, 205, 199, 117, 215,
            53, 70, 237, 35, 172, 197, 159, 249, 40, 76, 205, 49, 208, 14, 123,
            169, 145, 252, 96, 128, 142, 96, 26, 2, 128, 79, 6, 59, 90, 29,
            133, 161, 26, 217, 244, 230, 2, 53, 224, 210, 246, 153, 137, 90,
            123, 46, 111, 169, 209, 5, 93, 184, 158, 81, 112, 104, 77, 98, 147,
            13, 88, 169, 232, 28, 220, 93, 179, 239, 177, 2, 196, 57, 122, 213,
            103, 68, 167, 252, 146, 255, 144, 111, 89, 213, 151, 72, 143, 169,
            64, 12, 154, 222, 11, 123, 23, 151, 236, 235, 95, 133, 60, 150,
        ];

        pub fn signing_package() -> frost::SigningPackage {
            frost::SigningPackage::deserialize(SIGNING_PACKAGE).unwrap()
        }

        pub fn public_key_package() -> PublicKeyPackage {
            PublicKeyPackage::deserialize(PUBLIC_KEY_PACKAGE).unwrap()
        }

        pub fn fed_members() -> BTreeMap<frost::Identifier, secp256k1::PublicKey>
        {
            let secp = secp256k1::Secp256k1::new();
            let coordinator = coordinator();

            let seckey = secp256k1::SecretKey::new(&mut rand::thread_rng());
            let pubkey = secp256k1::PublicKey::from_secret_key(&secp, &seckey);

            let mut members = BTreeMap::new();
            members.insert(coordinator, pubkey);
            members
        }

        pub fn coordinator() -> frost::Identifier {
            frost::Identifier::try_from(1u16).unwrap()
        }
    }

    #[test]
    fn attestation_requires_staged_not_found() {
        let (mut manager, _submitter) = MultisigManager::new();

        // No multisig registered.
        let result = manager.recv(Message::Attestation {
            multisig_id: MultisigId::new(1),
            public_key_package: test_data::public_key_package(),
            signing_package: test_data::signing_package(),
            signatures: BTreeMap::new(),
        });

        assert!(matches!(result, Err(Error::MultisigIdNotExist)));
    }

    #[test]
    fn attestation_requires_staged_not_active() {
        let (mut manager, _submitter) = MultisigManager::new();
        let multisig_id = MultisigId::new(1);

        // Set as Active
        manager.set_active(
            multisig_id,
            test_data::coordinator(),
            test_data::fed_members(),
            test_data::public_key_package(),
        );

        let result = manager.recv(Message::Attestation {
            multisig_id,
            public_key_package: test_data::public_key_package(),
            signing_package: test_data::signing_package(),
            signatures: BTreeMap::new(),
        });

        assert!(matches!(result, Err(Error::LifecycleMustBeStaged)));
    }

    #[test]
    fn attestation_requires_staged_not_sunset() {
        let (mut manager, _submitter) = MultisigManager::new();
        let multisig_id = MultisigId::new(1);

        // Set as Sunset
        manager.set_sunset(
            multisig_id,
            test_data::coordinator(),
            test_data::fed_members(),
            test_data::public_key_package(),
        );

        let result = manager.recv(Message::Attestation {
            multisig_id,
            public_key_package: test_data::public_key_package(),
            signing_package: test_data::signing_package(),
            signatures: BTreeMap::new(),
        });

        assert!(matches!(result, Err(Error::LifecycleMustBeStaged)));
    }

    #[test]
    fn expiration_requires_sunset_not_found() {
        let (mut manager, _submitter) = MultisigManager::new();

        // No multisig registered
        let result = manager.recv(Message::Expiration {
            multisig_id: MultisigId::new(1),
            coordinator_signature: secp256k1::ecdsa::Signature::from_compact(
                &[1u8; 64],
            )
            .unwrap(),
        });

        assert!(matches!(result, Err(Error::MultisigIdNotExist)));
    }

    #[test]
    fn expiration_requires_sunset_not_staged() {
        let (mut manager, _submitter) = MultisigManager::new();
        let multisig_id = MultisigId::new(1);

        // Set as Staged
        manager.set_staged(
            multisig_id,
            test_data::coordinator(),
            test_data::fed_members(),
        );

        let result = manager.recv(Message::Expiration {
            multisig_id,
            coordinator_signature: secp256k1::ecdsa::Signature::from_compact(
                &[1u8; 64],
            )
            .unwrap(),
        });

        assert!(matches!(result, Err(Error::LifecycleMustBeSunset)));
    }

    #[test]
    fn expiration_requires_sunset_not_active() {
        let (mut manager, _submitter) = MultisigManager::new();
        let multisig_id = MultisigId::new(1);

        // Set as Active
        manager.set_active(
            multisig_id,
            test_data::coordinator(),
            test_data::fed_members(),
            test_data::public_key_package(),
        );

        let result = manager.recv(Message::Expiration {
            multisig_id,
            coordinator_signature: secp256k1::ecdsa::Signature::from_compact(
                &[1u8; 64],
            )
            .unwrap(),
        });

        assert!(matches!(result, Err(Error::LifecycleMustBeSunset)));
    }
}
