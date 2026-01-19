use bitcoin::secp256k1;
use encryption::{DkgHandshakeManager, SecureChannelManager};
use frost::keys::{
    dkg::{round1, round2},
    PublicKeyPackage,
};
use frost_secp256k1_tr::{self as frost, keys::KeyPackage};
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    fmt::Display,
    time::{Duration, Instant},
};
use thiserror::Error;
use uuid::Uuid;

use botanix_types::MultisigId;

use crate::dkg::encryption::AttestationManager;

mod encryption;
#[cfg(test)]
mod tests;

// NOTE: As of now, the session context is always the same constant. In
// the future this may change, and might be handled differently.
pub const SESSION_CONTEXT: &[u8] = b"static-dkg-session-context";

/// Wrapper type for FROST identifiers used in the DKG protocol.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
)]
pub struct Initiator(frost::Identifier);

#[cfg(test)]
impl From<frost::Identifier> for Initiator {
    fn from(id: frost::Identifier) -> Self {
        Initiator(id)
    }
}

/// Wrapper type for FROST identifiers used in the DKG protocol.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
)]
pub struct Target(frost::Identifier);

#[cfg(test)]
impl From<frost::Identifier> for Target {
    fn from(id: frost::Identifier) -> Self {
        Target(id)
    }
}

mod sealed_pkg {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SealedRoundOnePackage(round1::Package);

    impl SealedRoundOnePackage {
        pub fn new(
            package: round1::Package,
            auth: &mut DkgHandshakeManager,
        ) -> Result<
            (Self, secp256k1::PublicKey, secp256k1::ecdsa::Signature),
            encryption::Error,
        > {
            let (eph_pub, sig) = auth.commit_round1(&package)?;

            Ok((SealedRoundOnePackage(package), eph_pub, sig))
        }
        pub fn extract(
            self,
            initiator: Initiator,
            eph_pub: secp256k1::PublicKey,
            signature: secp256k1::ecdsa::Signature,
            auth: &mut DkgHandshakeManager,
        ) -> Result<round1::Package, encryption::Error> {
            auth.validate_round1(initiator, eph_pub, signature, &self.0)?;

            Ok(self.0)
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DynafedSubscriptionMessage {
    /// DKG-related notifications
    Dkg(DkgNotification),
    /// Migration-related notifications
    Migration(MigrationNotification),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DkgNotification {
    /// Start a new DKG session
    Start { multisig_id: MultisigId },
    /// Restart a DKG session
    Restart { multisig_id: MultisigId },
    /// Abort a DKG session
    Abort { multisig_id: MultisigId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationNotification {
    pub event: MigrationEvent,
    pub multisig_id_from: MultisigId,
    pub multisig_id_to: MultisigId,
    pub migration_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationEvent {
    Start,
    End,
    Abort,
}

// Helper methods
impl DkgNotification {
    pub fn multisig_id(&self) -> MultisigId {
        match self {
            DkgNotification::Start { multisig_id } => *multisig_id,
            DkgNotification::Restart { multisig_id } => *multisig_id,
            DkgNotification::Abort { multisig_id } => *multisig_id,
        }
    }
}

impl Display for DkgNotification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DkgNotification::Start { multisig_id } => {
                write!(f, "DKG Started {{ multisig_id: {} }}", multisig_id)
            }
            DkgNotification::Restart { multisig_id } => {
                write!(f, "DKG Restart {{ multisig_id: {} }}", multisig_id)
            }
            DkgNotification::Abort { multisig_id } => {
                write!(f, "DKG Aborted {{ multisig_id: {} }}", multisig_id)
            }
        }
    }
}

impl Display for MigrationNotification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Migration {:?} {{ from: {}, to: {}, id: {} }}",
            self.event,
            self.multisig_id_from,
            self.multisig_id_to,
            self.migration_id
        )
    }
}

impl Display for DynafedSubscriptionMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DynafedSubscriptionMessage::Dkg(dkg) => write!(f, "{}", dkg),
            DynafedSubscriptionMessage::Migration(migration) => {
                write!(f, "{}", migration)
            }
        }
    }
}

/// A message payload for the DKG protocol that contains routing information
/// and the actual message content.
///
/// This struct is passed between participants in the DKG protocol and contains
/// addressing information (sender and recipient) along with the specific DKG message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DkgPayload {
    /// The FROST identifier of the sender
    pub sender: frost::Identifier,
    /// The FROST identifier of the intended recipient
    pub recipient: frost::Identifier,
    /// The actual DKG message content
    pub msg: DkgMessage,
}

/// Represents the different types of messages exchanged during the DKG
/// protocol.
///
/// Each variant corresponds to a specific stage of the DKG protocol, either
/// sending cryptographic packages or acknowledging receipt of packages.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DkgMessage {
    /// A round1 message containing the sender's FROST round1 package.
    Round1 {
        /// The original initiator of this package
        initiator: Initiator,
        // The session context, dictated by the coordinator
        context: Vec<u8>,
        // The session nonce, dictated by the coordinator
        nonce: u64,
        /// The ephemeral public key
        ephemeral_pub: secp256k1::PublicKey,
        /// The signature of the round1 package
        signature: secp256k1::ecdsa::Signature,
        /// The round1 package containing commitments to secret values
        package: sealed_pkg::SealedRoundOnePackage,
    },
    /// Acknowledges receipt of a round1 message.
    AckRound1 {
        /// The initiator whose round1 message is being acknowledged
        initiator: Initiator,
    },
    /// A round2 message containing a FROST round2 package. This message
    /// includes a target identifier since round2 packages are specifically
    /// generated for each participant.
    Round2 {
        /// The original initiator of this package
        initiator: Initiator,
        /// The intended target of this package
        target: Target,
        /// The nonce used for encryption
        nonce: u64,
        /// The encrypted round2 package
        package: Vec<u8>,
    },
    /// Acknowledges receipt of a round2 message.
    AckRound2 {
        /// The initiator whose round2 message is being acknowledged
        initiator: Initiator,
        /// The target of the round2 message being acknowledged
        target: Target,
    },
    /// A round3 message containing the final, aggregated public key package.
    Round3 {
        /// The original initiator of this package
        initiator: Initiator,
        signing_commits: Option<frost::round1::SigningCommitments>,
    },
    /// Acknowledges receipt of a round3 message.
    AckRound3 {
        /// The initiator whose round3 message is being acknowledged
        initiator: Initiator,
    },
    Round4 {
        initiator: Initiator,
        signing_package: Option<frost::SigningPackage>,
        signature_share: frost::round2::SignatureShare,
        // TODO: add another sealed_pkg type for this?
        attestation_sig: secp256k1::ecdsa::Signature,
    },
    AckRound4 {
        initiator: Initiator,
    },
}

/// Represents a completed DKG attestation to be submitted on-chain.
///
/// This attestation is submitted on-chain via consensus, where all consensus
/// validators must:
///
/// 1. Validate each individual signature share in
///    [`signatures`](Self::signatures)
/// 2. Verify that the [`aggregated_signature`](Self::aggregated_signature)
///    (produced via `frost::aggregate`) corresponds to the
///    [`public_key_package`](Self::public_key_package)
///
/// For this validation to work reliably, consensus validators must have prior
/// knowledge of the federation membership, respetively which participants are
/// part of the DKG.
///
/// The message used for signature validation must be constructed according to
/// [`AttestationManager`](encryption::AttestationManager).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attestation {
    pub public_key_package: PublicKeyPackage,
    pub signing_package: frost::SigningPackage,
    pub signatures: BTreeMap<
        frost::Identifier,
        (frost::round2::SignatureShare, secp256k1::ecdsa::Signature),
    >,
    pub aggregated_signature: frost::Signature,
}

/// Configuration parameters for the DKG state machine.
///
/// This struct defines the key parameters for a threshold signing scheme
/// and timeout durations for the different rounds of the DKG protocol.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// The maximum number of signers (participants) in the threshold scheme.
    /// This is typically equal to the total number of participants.
    pub max_signers: u16,

    /// The minimum number of signers required to produce a valid signature.
    pub min_signers: u16,

    /// The timeout duration for round1 packages. If an acknowledgment isn't
    /// received within this duration, the package will be resent.
    pub round1_package_timeout: Duration,

    /// The timeout duration for round2 packages. If an acknowledgment isn't
    /// received within this duration, the package will be resent.
    pub round2_package_timeout: Duration,

    /// The timeout duration for round3 packages. If an acknowledgment isn't
    /// received within this duration, the package will be resent.
    pub round3_package_timeout: Duration,

    /// The timeout duration for round4 packages. If an acknowledgment isn't
    /// received within this duration, the package will be resent.
    pub round4_package_timeout: Duration,

    /// The optional timeout duration for the entire DKG session. If the session
    /// isn't completed within this duration, it will be reset. This increments
    /// the session nonce and creates new round1 packages.
    pub pending_session_timeout: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Represents the current stage of the DKG protocol.
pub enum Stage {
    /// Waiting for the coordinator to initialize the DKG session.
    AwaitingInit,
    /// Active exchange of round1 packages between participants.
    RoundOne,
    /// Active exchange of round2 packages between participants.
    RoundTwo,
    /// Active exchange of round3 packages (aggregated public keys).
    RoundThree,
    RoundFour,
    AwaitingRoundFour,
    /// The DKG process was aborted.
    Aborted,
    /// DKG protocol finalized successfully.
    Finalized,
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Stage::AwaitingInit => write!(f, "AwaitingInitialization"),
            Stage::RoundOne => write!(f, "RoundOne"),
            Stage::RoundTwo => write!(f, "RoundTwo"),
            Stage::RoundThree => write!(f, "RoundThree"),
            Stage::RoundFour => write!(f, "RoundFour"),
            Stage::AwaitingRoundFour => write!(f, "AwaitingRoundFour"),
            Stage::Aborted => write!(f, "Aborted"),
            Stage::Finalized => write!(f, "Finalized"),
        }
    }
}

trait HasTimer {
    fn timer(&self) -> Option<Instant>;
}

fn min_timer_optional<'a, K, T: HasTimer>(
    packages: impl Iterator<Item = &'a Option<T>>,
    now: Instant,
) -> Option<Duration>
where
    T: 'a,
{
    packages
        .filter_map(|e| e.as_ref())
        .filter_map(|e| e.timer())
        .map(|t| t.saturating_duration_since(now))
        .min()
}

fn min_timer<'a, K, T: HasTimer>(
    packages: impl Iterator<Item = &'a T>,
    now: Instant,
) -> Option<Duration>
where
    T: 'a,
{
    packages
        .filter_map(|e| e.timer())
        .map(|t| t.saturating_duration_since(now))
        .min()
}

#[derive(Debug, Clone)]
struct OutEntryRoundOne {
    package: sealed_pkg::SealedRoundOnePackage,
    ephemeral_pub: secp256k1::PublicKey,
    signature: secp256k1::ecdsa::Signature,
    timer: Option<Instant>,
    attempts: usize,
}

impl HasTimer for OutEntryRoundOne {
    fn timer(&self) -> Option<Instant> {
        self.timer
    }
}

#[derive(Debug, Clone)]
struct OutEntryRoundTwo {
    ciphernonce: u64,
    ciphertext: Vec<u8>,
    timer: Option<Instant>,
    attempts: usize,
}

impl HasTimer for OutEntryRoundTwo {
    fn timer(&self) -> Option<Instant> {
        self.timer
    }
}

#[derive(Debug, Clone)]
struct OutEntryRoundThree {
    signing_commits: frost::round1::SigningCommitments,
    timer: Option<Instant>,
    attempts: usize,
}

impl HasTimer for OutEntryRoundThree {
    fn timer(&self) -> Option<Instant> {
        self.timer
    }
}

#[derive(Debug, Clone)]
struct OutEntryRoundFour {
    signature_share: frost::round2::SignatureShare,
    attestation_sig: secp256k1::ecdsa::Signature,
    timer: Option<Instant>,
    attempts: usize,
}

impl HasTimer for OutEntryRoundFour {
    fn timer(&self) -> Option<Instant> {
        self.timer
    }
}

#[derive(Debug)]
/// Represents the current state of the DKG process.
enum StageState {
    /// The DKG process is waiting for the coordinator to initialize the session.
    AwaitingInit,
    /// The active stage of round1. Participants are exchanging round1 packages
    /// with each other. The coordinator distributes packages between all
    /// participants.
    RoundOne {
        context: Vec<u8>,
        nonce: u64,
        auth: DkgHandshakeManager,
        secret_package: round1::SecretPackage,
        in_round1_packages: BTreeMap<Initiator, round1::Package>,
        out_round1_packages:
            BTreeMap<(Initiator, frost::Identifier), Option<OutEntryRoundOne>>,
    },
    /// The active stage of round2. Participants are exchanging round2 packages
    /// with each other. The coordinator distributes packages between all
    /// participants and tracks which packages have been distributed through the
    /// dist_checklist.
    RoundTwo {
        pending: bool,
        auth: SecureChannelManager,
        secret_package: round2::SecretPackage,
        in_round1_packages: BTreeMap<frost::Identifier, round1::Package>,
        in_round2_packages: BTreeMap<Initiator, round2::Package>,
        out_round2_packages:
            BTreeMap<(Initiator, Target), Option<OutEntryRoundTwo>>,
    },
    /// The active stage of round3. Participants are exchanging aggregated
    /// public key packages to ensure everyone has the same final, aggregated
    /// public key.
    RoundThree {
        // TODO: Needed?
        pending: bool,
        dkg_commit: [u8; 32],
        secret_package: frost::keys::KeyPackage,
        public_key_package: PublicKeyPackage,
        signing_nonces: frost::round1::SigningNonces,
        in_round3_packages:
            BTreeMap<frost::Identifier, frost::round1::SigningCommitments>,
        out_round3_packages: BTreeMap<frost::Identifier, OutEntryRoundThree>,
    },
    AwaitingRoundFour {
        dkg_commit: [u8; 32],
        secret_package: frost::keys::KeyPackage,
        public_key_package: PublicKeyPackage,
        signing_nonces: frost::round1::SigningNonces,
    },
    RoundFour {
        auth: AttestationManager,
        signing_package: frost::SigningPackage,
        secret_package: frost::keys::KeyPackage,
        public_key_package: PublicKeyPackage,
        in_round4_packages: BTreeMap<
            Initiator,
            (frost::round2::SignatureShare, secp256k1::ecdsa::Signature),
        >,
        out_round4_packages:
            BTreeMap<(Initiator, frost::Identifier), Option<OutEntryRoundFour>>,
    },
    /// The DKG process was aborted. This can happen if FROST fails to generate
    /// new rounds, for example if a peer provided an incorrect or malformed
    /// package.
    // TODO: This is not really used?
    Aborted,
    /// The DKG process completed successfully. All participants have verified
    /// they have the same public key package.
    Finalized {
        secret_package: frost::keys::KeyPackage,
        public_key_package: PublicKeyPackage,
        signing_package: frost::SigningPackage,
        aggregated_sig: frost::Signature,
        attestations: BTreeMap<
            frost::Identifier,
            (frost::round2::SignatureShare, secp256k1::ecdsa::Signature),
        >,
    },
}

impl StageState {
    fn did_round_four_finalize(&self) -> bool {
        matches!(self, StageState::Finalized { .. })
    }
    fn did_round_three_finalize(&self) -> bool {
        matches!(
            self,
            StageState::RoundFour { .. } | StageState::AwaitingRoundFour { .. }
        ) || self.did_round_four_finalize()
    }
    fn did_round_two_finalize(&self) -> bool {
        matches!(self, StageState::RoundThree { .. })
            || self.did_round_three_finalize()
    }
    fn did_round_one_finalize(&self) -> bool {
        matches!(self, StageState::RoundTwo { .. })
            || self.did_round_two_finalize()
    }
}

/// A queue for outgoing DKG payloads.
struct Queue {
    i: VecDeque<DkgPayload>,
    my_frost_id: frost::Identifier,
}

impl Queue {
    fn send_round1_ack(
        &mut self,
        initiator: Initiator,
        recipient: frost::Identifier,
    ) {
        let msg = DkgPayload {
            sender: self.my_frost_id,
            recipient,
            msg: DkgMessage::AckRound1 { initiator },
        };

        self.i.push_back(msg);
    }
    fn send_round2_ack(
        &mut self,
        initiator: Initiator,
        recipient: frost::Identifier,
        target: Target,
    ) {
        let msg = DkgPayload {
            sender: self.my_frost_id,
            recipient,
            msg: DkgMessage::AckRound2 { initiator, target },
        };

        self.i.push_back(msg);
    }

    fn send_round3_ack(
        &mut self,
        initiator: Initiator,
        recipient: frost::Identifier,
    ) {
        let msg = DkgPayload {
            sender: self.my_frost_id,
            recipient,
            msg: DkgMessage::AckRound3 { initiator },
        };

        self.i.push_back(msg);
    }

    fn send_round4_ack(
        &mut self,
        initiator: Initiator,
        recipient: frost::Identifier,
    ) {
        let msg = DkgPayload {
            sender: self.my_frost_id,
            recipient,
            msg: DkgMessage::AckRound4 { initiator },
        };

        self.i.push_back(msg);
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Bad DKG configuration: {0}")]
    BadConfig(String),
    #[error("Frost error: {0}")]
    Frost(#[from] frost::Error),
    #[error("Encryption error: {0}")]
    Encryption(#[from] encryption::Error),
}

/// The DKG state machine handles distributed key generation across multiple participants.
pub struct DkgStateMachine {
    config: Config,
    my_frost_id: frost::Identifier,
    my_static_sec: secp256k1::SecretKey,
    multisig_id: MultisigId,
    coordinator: frost::Identifier,
    members: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
    queue: Queue,
    state: StageState,
    session_nonce: Option<u64>,
    session_activated: Option<Instant>,
}

impl DkgStateMachine {
    /// Creates a new DKG state machine.
    ///
    /// This constructor initializes a new DKG state machine with the specified
    /// participant identifiers and configuration parameters.
    ///
    /// When created, the state machine behavior differs based on whether this participant
    /// is the coordinator:
    ///
    /// - If this participant is the coordinator (`my_frost_id == coordinator`), the state machine
    ///   enters the `RoundOne` stage by immediately sending the round1 packages to all other
    ///   participants. These initial packages can be retrieved by calling `send()`.
    ///
    /// - If this participant is not the coordinator, the state machine sets the `pending` flag to
    ///   `true`. It generates its round1 package but doesn't send it yet. Instead, it waits to
    ///   receive the coordinator's round1 package before becoming active and sending any messages.
    ///
    /// # Arguments
    ///
    /// * `my_frost_id` - The FROST identifier of this participant.
    /// * `coordinator` - The FROST identifier of the designated coordinator.
    /// * `members` - A list of all participant FROST identifiers in the DKG process, including this
    ///   participant and the coordinator.
    /// * `config` - Configuration parameters for the DKG process.
    /// * `session_nonce` - An increasing value that uniquely identifies this DKG session. Only the
    ///   coordinator should provide this value (typically using Unix time in seconds).
    ///   Non-coordinators should pass `None` and will accept the nonce from the coordinator's
    ///   initial message. The nonce increases automatically when sessions timeout and restart.
    pub fn new(
        my_frost_id: frost::Identifier,
        my_static_sec: secp256k1::SecretKey,
        multisig_id: MultisigId,
        coordinator: frost::Identifier,
        members: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
        config: Config,
        session_nonce: Option<u64>,
    ) -> Result<Self, Error> {
        if !members.contains_key(&my_frost_id) {
            return Err(Error::BadConfig(
                "my_frost_id not in members".to_string(),
            ));
        }

        if !members.contains_key(&coordinator) {
            return Err(Error::BadConfig(
                "coordinator not in members".to_string(),
            ));
        }

        if config.min_signers > config.max_signers {
            return Err(Error::BadConfig(
                "min_signers cannot be greater than max_signers".to_string(),
            ));
        }

        if (members.len() as u16) != config.max_signers {
            return Err(Error::BadConfig(
                "max_signers does not match member size".to_string(),
            ));
        }

        if my_frost_id == coordinator && session_nonce.is_none() {
            return Err(Error::BadConfig(
                "coordinator must provide a session nonce".to_string(),
            ));
        }

        // Setup sending queue.
        let queue = Queue { i: VecDeque::new(), my_frost_id };

        let mut this = Self {
            config,
            my_frost_id,
            my_static_sec,
            multisig_id,
            coordinator,
            members,
            queue,
            state: StageState::AwaitingInit,
            session_nonce: None,
            session_activated: None,
        };

        // The coordinator immediately starts the DKG process by sending a
        // `DkgMessage::NewSession` followed by the actual `DkgMessage::Round1`
        // package to each participant.
        //
        // Non-coordinators wait for the coordinator to send this message, and
        // initialize the round1 package and authentication layer then.
        if this.is_coordinator() {
            let nonce = session_nonce.expect("session nonce must be provided");
            this.init_new_session(nonce)?;

            debug_assert_eq!(this.stage(), Stage::RoundOne);
        }

        Ok(this)
    }
    /// Initializes a new DKG session with the given nonce by resetting the
    /// stage, setting up a new authentication layer, generating a new round1
    /// package, and sending it to all participants.
    ///
    /// Only the coordinator must call this method.
    fn init_new_session(&mut self, nonce: u64) -> Result<(), Error> {
        debug_assert!(self.is_coordinator());

        let context = SESSION_CONTEXT.to_vec();
        //
        self.session_activated = None;
        self.session_nonce = Some(nonce);
        self.queue.i.clear();

        // Generate the secret package and our round1 package
        let (secret_package, our_round1_package) = frost::keys::dkg::part1(
            self.my_frost_id,
            self.config.max_signers,
            self.config.min_signers,
            thread_rng(),
        )?;

        // AUTHENTICATION: Setup authentication and encryption layer.
        let mut auth = DkgHandshakeManager::new(
            &context,
            nonce,
            self.my_frost_id,
            self.my_static_sec,
            self.members.clone(),
        )?;

        // Seal our round1 package.
        let (sealed_round1_package, our_eph_pub, our_sig) =
            sealed_pkg::SealedRoundOnePackage::new(
                our_round1_package,
                &mut auth,
            )?;

        let out_entry = OutEntryRoundOne {
            package: sealed_round1_package.clone(),
            ephemeral_pub: our_eph_pub,
            signature: our_sig,
            timer: None,
            attempts: 0,
        };

        let mut out_round1_packages = BTreeMap::new();

        // Prepare all outgoing round1 package entries that we need to have
        // acknowledged, including forwarded messages.
        //
        // For example; with three participants Alice (us), Bob, and Eve, we construct:
        // * Alice -> Bob
        // * Alice -> Eve
        // * Bob -> Eve (forwarded)
        // * Eve -> Bob (forwarded)
        for initiator in self.members.keys().copied() {
            for recipient in self.members.keys().copied() {
                // Skip ourself.
                if recipient == self.my_frost_id {
                    continue;
                }

                if initiator == recipient {
                    continue;
                }

                // Only set our packages; forwarded packages are set once
                // they're received, of course.
                let out_entry = if initiator == self.my_frost_id {
                    Some(out_entry.clone())
                } else {
                    None
                };

                // Track outgoing package.
                out_round1_packages
                    .insert((Initiator(initiator), recipient), out_entry);

                if initiator != self.my_frost_id {
                    // Skip sending unless it's us.
                    continue;
                }

                // Push the round1 payload to the queue.
                let msg = DkgPayload {
                    sender: self.my_frost_id,
                    recipient,
                    msg: DkgMessage::Round1 {
                        initiator: Initiator(self.my_frost_id),
                        context: context.clone(),
                        nonce,
                        ephemeral_pub: our_eph_pub,
                        signature: our_sig,
                        package: sealed_round1_package.clone(),
                    },
                };

                self.queue.i.push_back(msg);
            }
        }

        self.state = StageState::RoundOne {
            nonce,
            auth,
            secret_package,
            in_round1_packages: BTreeMap::new(),
            out_round1_packages,
            context,
        };

        Ok(())
    }
    /// Returns the FROST identifier of this participant.
    pub fn frost_id(&self) -> frost::Identifier {
        self.my_frost_id
    }
    /// Returns the multisig identifier for this DKG session.
    pub fn multisig_id(&self) -> MultisigId {
        self.multisig_id
    }
    /// Checks if this participant is the coordinator of the DKG process.
    pub fn is_coordinator(&self) -> bool {
        self.my_frost_id == self.coordinator
    }
    /// Returns the current stage of the DKG protocol.
    pub fn stage(&self) -> Stage {
        match &self.state {
            StageState::AwaitingInit => Stage::AwaitingInit,
            StageState::RoundOne { .. } => Stage::RoundOne,
            StageState::RoundTwo { .. } => Stage::RoundTwo,
            StageState::RoundThree { .. } => Stage::RoundThree,
            StageState::RoundFour { .. } => Stage::RoundFour,
            StageState::AwaitingRoundFour { .. } => Stage::AwaitingRoundFour,
            StageState::Finalized { .. } => Stage::Finalized,
            StageState::Aborted => Stage::Aborted,
        }
    }
    /// Returns the session nonce, if available.
    pub fn session_nonce(&self) -> Option<u64> {
        self.session_nonce
    }
    /// Returns the aggregated key packages if they are available.
    ///
    /// The key packages become available after the second round completes,
    /// respectively from round three onwards. When available, the key packages
    /// should be persisted to disk immediately. However, the DKG process **is
    /// NOT** considered complete until [`attestation`](Self::attestation)
    /// returns a value.
    //
    // TODO: Remove `aggregate_*` prefix?
    pub fn aggregate_key_packages(
        &self,
    ) -> Option<(&KeyPackage, &PublicKeyPackage)> {
        match &self.state {
            StageState::RoundThree {
                secret_package,
                public_key_package,
                ..
            }
            | StageState::AwaitingRoundFour {
                secret_package,
                public_key_package,
                ..
            }
            | StageState::RoundFour {
                secret_package,
                public_key_package,
                ..
            }
            | StageState::Finalized {
                secret_package,
                public_key_package,
                ..
            } => Some((secret_package, public_key_package)),
            _ => None,
        }
    }
    /// Returns the attestation if the DKG process has completed successfully.
    ///
    /// When this method returns a value, the DKG process is considered
    /// complete. At this point, the key packages retrieved via
    /// [`aggregate_key_packages`](Self::aggregate_key_packages) should already
    /// be stored on disk in a persistent manner.
    pub fn attestation(&self) -> Option<Attestation> {
        if let StageState::Finalized {
            public_key_package,
            signing_package,
            attestations,
            aggregated_sig,
            ..
        } = &self.state
        {
            Some(Attestation {
                public_key_package: public_key_package.clone(),
                signing_package: signing_package.clone(),
                signatures: attestations.clone(),
                aggregated_signature: aggregated_sig.clone(),
            })
        } else {
            None
        }
    }
    /// Returns the duration until the next timeout event, if any.
    ///
    /// This method should be called after any state-changing operation,
    /// particularly after calling `send()`. The returned duration indicates how
    /// long to wait before calling `on_timeout()`.
    ///
    /// # Arguments
    ///
    /// * `now` - The current time
    ///
    /// # Returns
    ///
    /// An optional `Duration` until the next timeout event. If `None`, there
    /// are no pending timeout events.
    pub fn timeout(&self, now: Instant) -> Option<Duration> {
        let mut session_timeout = None;

        // If we're the coordinator and the DKG session has not been finalized...
        // TODO: session-timeout should be reset on each stage transition!
        if self.is_coordinator() && self.stage() != Stage::Finalized {
            // And if a max session timeout has been configured...
            if let Some(max) = self.config.pending_session_timeout {
                // And if a DKG session has started...
                if let Some(session_activated) = self.session_activated {
                    let t = (session_activated + max)
                        .saturating_duration_since(now);
                    session_timeout = Some(t);
                }
            }
        }

        let t = match &self.state {
            StageState::RoundOne { out_round1_packages, .. } => {
                min_timer_optional::<(), _>(out_round1_packages.values(), now)
            }
            StageState::RoundTwo { out_round2_packages, .. } => {
                min_timer_optional::<(), _>(out_round2_packages.values(), now)
            }
            StageState::RoundThree { out_round3_packages, .. } => {
                min_timer::<(), _>(out_round3_packages.values(), now)
            }
            StageState::RoundFour { out_round4_packages, .. } => {
                min_timer_optional::<(), _>(out_round4_packages.values(), now)
            }
            _ => None,
        };

        match (t, session_timeout) {
            (Some(t), Some(s)) => Some(t.min(s)),
            (Some(t), None) => Some(t),
            (None, Some(x)) => Some(x),
            (None, None) => None,
        }
    }
    /// Processes timeout events for outgoing messages.
    ///
    /// This method should be called when the duration returned by `timeout()`
    /// expires. It will trigger re-sending of any messages that haven't been
    /// acknowledged. After calling this method, you should call `send()` in a
    /// loop to get any payloads that need to be re-sent.
    ///
    /// # Arguments
    ///
    /// * `now` - The current time
    pub fn on_timeout(&mut self, now: Instant) {
        let self_is_coordinator = self.is_coordinator();

        // If we're the coordinator and the DKG session has not been finalized...
        if self_is_coordinator && self.stage() != Stage::Finalized {
            // And if a max session timeout has been configured...
            if let Some(max) = self.config.pending_session_timeout {
                // And if a DKG session has started...
                if let Some(session_activated) = self.session_activated {
                    // And if the timeout has expired...
                    if now >= session_activated + max {
                        // Increment nonce.
                        let nonce = self
                            .session_nonce
                            .as_ref()
                            .expect("nonce tracker must be set");
                        let nonce = nonce.wrapping_add(1);

                        // Start a new session.
                        self.init_new_session(nonce)
                            .expect("failed to init new session");
                    }
                }
            }
        }

        // Helper to check if a timer has expired.
        let timer_expired = |timer: Option<Instant>| -> bool {
            timer.is_some_and(|t| t <= now + Duration::from_millis(1))
        };

        match &mut self.state {
            StageState::RoundOne {
                context,
                nonce,
                out_round1_packages,
                ..
            } => {
                for ((initiator, recipient), entry) in
                    out_round1_packages.iter()
                {
                    let Some(entry) = entry else { continue };
                    if !timer_expired(entry.timer) {
                        continue;
                    }

                    self.queue.i.push_back(DkgPayload {
                        sender: self.my_frost_id,
                        recipient: *recipient,
                        msg: DkgMessage::Round1 {
                            initiator: *initiator,
                            context: context.clone(),
                            nonce: *nonce,
                            ephemeral_pub: entry.ephemeral_pub,
                            signature: entry.signature,
                            package: entry.package.clone(),
                        },
                    });
                }
            }
            StageState::RoundTwo { out_round2_packages, .. } => {
                for ((initiator, target), entry) in out_round2_packages.iter() {
                    let Some(entry) = entry else { continue };
                    if !timer_expired(entry.timer) {
                        continue;
                    }

                    // TODO: Comment on this
                    let recipient = if self_is_coordinator {
                        target.0
                    } else {
                        self.coordinator
                    };

                    self.queue.i.push_back(DkgPayload {
                        sender: self.my_frost_id,
                        recipient,
                        msg: DkgMessage::Round2 {
                            initiator: *initiator,
                            target: *target,
                            nonce: entry.ciphernonce,
                            package: entry.ciphertext.clone(),
                        },
                    });
                }
            }
            StageState::RoundThree { out_round3_packages, .. } => {
                for (recipient, entry) in out_round3_packages.iter() {
                    if !timer_expired(entry.timer) {
                        continue;
                    }

                    self.queue.i.push_back(DkgPayload {
                        sender: self.my_frost_id,
                        recipient: *recipient,
                        msg: DkgMessage::Round3 {
                            initiator: Initiator(self.my_frost_id),
                            signing_commits: Some(entry.signing_commits),
                        },
                    });
                }
            }
            StageState::RoundFour { out_round4_packages, .. } => {
                for ((initiator, recipient), entry) in
                    out_round4_packages.iter()
                {
                    let Some(entry) = entry else { continue };
                    if !timer_expired(entry.timer) {
                        continue;
                    }

                    self.queue.i.push_back(DkgPayload {
                        sender: self.my_frost_id,
                        recipient: *recipient,
                        msg: DkgMessage::Round4 {
                            initiator: *initiator,
                            signing_package: None,
                            signature_share: entry.signature_share,
                            attestation_sig: entry.attestation_sig,
                        },
                    });
                }
            }
            _ => {}
        }
    }
    /// Returns the next outgoing payload, if any.
    ///
    /// This method should be called after any state changes, respectively after
    /// calling `recv()` or `on_timeout()`. It may return multiple payloads, so
    /// it should be called in a loop until it returns `None`.
    ///
    /// # Arguments
    ///
    /// * `now` - The current time, used for setting timeouts for outgoing messages
    ///
    /// # Returns
    ///
    /// An optional `DkgPayload` to be sent to another participant
    pub fn send(&mut self, now: Instant) -> Option<DkgPayload> {
        loop {
            let payload = self.queue.i.pop_front()?;

            match payload.msg {
                DkgMessage::Round1 { initiator, .. } => {
                    let StageState::RoundOne { out_round1_packages, .. } =
                        &mut self.state
                    else {
                        continue;
                    };
                    let Some(Some(entry)) = out_round1_packages
                        .get_mut(&(initiator, payload.recipient))
                    else {
                        continue;
                    };

                    if self.session_activated.is_none() {
                        self.session_activated = Some(now);
                    }

                    entry.timer =
                        Some(now + self.config.round1_package_timeout);
                    entry.attempts += 1;
                }
                DkgMessage::Round2 { initiator, target, .. } => {
                    let StageState::RoundTwo { out_round2_packages, .. } =
                        &mut self.state
                    else {
                        continue;
                    };
                    let Some(Some(entry)) =
                        out_round2_packages.get_mut(&(initiator, target))
                    else {
                        continue;
                    };

                    entry.timer =
                        Some(now + self.config.round2_package_timeout);
                    entry.attempts += 1;
                }
                // TODO: Note why we don't bother with the `initiator` here
                DkgMessage::Round3 { .. } => {
                    let StageState::RoundThree { out_round3_packages, .. } =
                        &mut self.state
                    else {
                        continue;
                    };
                    let Some(entry) =
                        out_round3_packages.get_mut(&payload.recipient)
                    else {
                        continue;
                    };

                    entry.timer =
                        Some(now + self.config.round3_package_timeout);
                    entry.attempts += 1;
                }
                DkgMessage::Round4 { initiator, .. } => {
                    let StageState::RoundFour { out_round4_packages, .. } =
                        &mut self.state
                    else {
                        continue;
                    };
                    let Some(Some(entry)) = out_round4_packages
                        .get_mut(&(initiator, payload.recipient))
                    else {
                        continue;
                    };

                    entry.timer =
                        Some(now + self.config.round4_package_timeout);
                    entry.attempts += 1;
                }
                _ => {}
            }

            return Some(payload);
        }
    }
    /// Processes an incoming payload from another participant.
    ///
    /// This method updates the internal state based on the received payload.
    /// After calling this method, you should call `send()` in a loop to get any
    /// response payloads that need to be sent.
    ///
    /// # Arguments
    ///
    /// * `payload` - The incoming payload from another participant
    pub fn recv(&mut self, payload: DkgPayload) -> Result<(), Error> {
        if payload.recipient != self.my_frost_id {
            return Ok(());
        }

        let DkgPayload { sender, msg, .. } = payload;

        match msg {
            DkgMessage::Round1 {
                initiator,
                context,
                nonce,
                ephemeral_pub,
                signature,
                package,
                ..
            } => {
                self.on_dkg_msg_round1(
                    initiator,
                    context,
                    nonce,
                    ephemeral_pub,
                    signature,
                    package,
                    sender,
                )?;
                self.transition_stage2_checked()?;
            }
            DkgMessage::AckRound1 { initiator } => {
                self.on_dkg_msg_ack_round1(initiator, sender)?;
                self.transition_stage2_checked()?;
            }
            DkgMessage::Round2 { initiator, target, nonce, package } => {
                self.on_dkg_msg_round2(
                    initiator, target, nonce, package, sender,
                )?;
                self.transition_stage3_checked()?;
            }
            DkgMessage::AckRound2 { initiator, target } => {
                self.on_dkg_msg_ack_round2(initiator, target)?;
                self.transition_stage3_checked()?;
            }
            DkgMessage::Round3 { initiator, signing_commits } => {
                self.on_dkg_msg_round3(initiator, signing_commits, sender)?;
                self.transition_stage4_checked()?;
            }
            DkgMessage::AckRound3 { initiator } => {
                self.on_dkg_msg_ack_round3(initiator, sender)?;
                self.transition_stage4_checked()?;
            }
            DkgMessage::Round4 {
                initiator,
                signing_package,
                signature_share,
                attestation_sig,
            } => {
                self.on_dkg_msg_round4(
                    initiator,
                    signing_package,
                    signature_share,
                    attestation_sig,
                    sender,
                )?;
                self.transition_final_checked()?;
            }
            DkgMessage::AckRound4 { initiator } => {
                self.on_dkg_msg_ack_round4(initiator, sender)?;
                self.transition_final_checked()?;
            }
        }

        Ok(())
    }
    fn on_dkg_msg_ack_round1(
        &mut self,
        initiator: Initiator,
        sender: frost::Identifier,
    ) -> Result<(), Error> {
        if !self.members.contains_key(&initiator.0) {
            return Ok(());
        }

        let StageState::RoundOne { out_round1_packages, .. } = &mut self.state
        else {
            // Ignore
            return Ok(());
        };

        out_round1_packages.remove(&(initiator, sender));

        Ok(())
    }
    fn on_dkg_msg_ack_round2(
        &mut self,
        initiator: Initiator,
        target: Target,
    ) -> Result<(), Error> {
        if !self.members.contains_key(&initiator.0) {
            return Ok(());
        }

        let StageState::RoundTwo { out_round2_packages, .. } = &mut self.state
        else {
            // Ignore
            return Ok(());
        };

        out_round2_packages.remove(&(initiator, target));

        Ok(())
    }
    fn on_dkg_msg_ack_round3(
        &mut self,
        initiator: Initiator,
        sender: frost::Identifier,
    ) -> Result<(), Error> {
        if !self.members.contains_key(&initiator.0) {
            return Ok(());
        }

        let StageState::RoundThree { out_round3_packages, .. } =
            &mut self.state
        else {
            // Ignore
            return Ok(());
        };

        out_round3_packages.remove(&sender);

        Ok(())
    }
    fn on_dkg_msg_ack_round4(
        &mut self,
        initiator: Initiator,
        sender: frost::Identifier,
    ) -> Result<(), Error> {
        if !self.members.contains_key(&initiator.0) {
            return Ok(());
        }

        let StageState::RoundFour { out_round4_packages, .. } = &mut self.state
        else {
            // Ignore
            return Ok(());
        };

        out_round4_packages.remove(&(initiator, sender));

        Ok(())
    }
    /// Processes a new session initialization as request by the coordinator by
    /// validating the request, resetting the stage, setting up a new
    /// authentication layer, generating a new round1 package, and sending it to
    /// the coordinator. Returns `true` if the session was successfully
    /// initialized.
    ///
    /// Only non-coordinators should call this method.
    fn validate_new_session_init(
        &mut self,
        initiator: Initiator,
        their_context: Vec<u8>,
        their_nonce: u64,
        their_ephmeral_pub: secp256k1::PublicKey,
        their_signature: secp256k1::ecdsa::Signature,
        their_package: sealed_pkg::SealedRoundOnePackage,
    ) -> Result<bool, Error> {
        if self.is_coordinator() {
            return Ok(false);
        }

        if initiator.0 != self.coordinator {
            return Ok(false);
        }

        if their_context != SESSION_CONTEXT {
            return Ok(false);
        }

        if let Some(last) = self.session_nonce {
            if last >= their_nonce {
                // Outdated or active nonce, ignore.
                return Ok(false);
            }
        }

        // Cannot start a new session if the DKG process has already been
        // finalized.
        if self.stage() == Stage::Finalized {
            return Ok(false);
        }

        // AUTHENTICATION: Setup new session parameters as-is. We then validate
        // the coordinators' round1 package, which is inherently tied to those
        // parameters.
        let mut auth = DkgHandshakeManager::new(
            &their_context,
            their_nonce,
            self.my_frost_id,
            self.my_static_sec,
            self.members.clone(),
        )?;

        let their_package = their_package.extract(
            initiator,
            their_ephmeral_pub,
            their_signature,
            &mut auth,
        )?;

        // Verification succeeded, the new session is accepted!

        // Set new session params.
        let context = their_context;
        let nonce = their_nonce;
        //
        self.session_activated = None;
        self.session_nonce = Some(nonce);
        self.queue.i.clear();

        // Track the coordinators' round1 package.
        let mut in_round1_packages = BTreeMap::new();
        in_round1_packages.insert(initiator, their_package.clone());

        self.queue.send_round1_ack(initiator, self.coordinator);

        // Generate the new secret package and our round1 package
        let (secret_package, our_round1_package) = frost::keys::dkg::part1(
            self.my_frost_id,
            self.config.max_signers,
            self.config.min_signers,
            thread_rng(),
        )?;

        // Seal our round1 package.
        let (sealed_round1_package, our_eph_pub, our_sig) =
            sealed_pkg::SealedRoundOnePackage::new(
                our_round1_package.clone(),
                &mut auth,
            )?;

        let out_entry = OutEntryRoundOne {
            package: sealed_round1_package.clone(),
            ephemeral_pub: our_eph_pub,
            signature: our_sig,
            timer: None,
            attempts: 0,
        };

        let mut out_round1_packages = BTreeMap::new();

        // Non-coordinators only have one outgoing package to send (to the
        // coordinator).
        out_round1_packages.insert(
            (Initiator(self.my_frost_id), self.coordinator),
            Some(out_entry),
        );

        // Push the pending, outgoing payload to the queue.
        let msg = DkgPayload {
            sender: self.my_frost_id,
            recipient: self.coordinator,
            msg: DkgMessage::Round1 {
                context: context.clone(),
                nonce,
                initiator: Initiator(self.my_frost_id),
                ephemeral_pub: our_eph_pub,
                signature: our_sig,
                package: sealed_round1_package,
            },
        };

        self.queue.i.push_back(msg);

        self.state = StageState::RoundOne {
            context,
            nonce,
            auth,
            secret_package,
            in_round1_packages,
            out_round1_packages,
        };

        Ok(true)
    }
    #[allow(clippy::too_many_arguments)]
    fn on_dkg_msg_round1(
        &mut self,
        initiator: Initiator,
        their_context: Vec<u8>,
        their_nonce: u64,
        their_ephmeral_pub: secp256k1::PublicKey,
        their_signature: secp256k1::ecdsa::Signature,
        their_package: sealed_pkg::SealedRoundOnePackage,
        sender: frost::Identifier,
    ) -> Result<(), Error> {
        if !self.members.contains_key(&initiator.0) {
            return Ok(());
        }

        if initiator.0 == self.my_frost_id {
            return Ok(());
        }

        if their_context != SESSION_CONTEXT {
            return Ok(());
        }

        // Do a simple check to determine whether we should start the new
        // session transition process.
        let do_new_session_check = if let Some(last) = self.session_nonce {
            match their_nonce.cmp(&last) {
                std::cmp::Ordering::Greater => {
                    // The package is for a new session, do check.
                    true
                }
                std::cmp::Ordering::Equal => {
                    // The package is for the active session, ignore check.
                    false
                }
                std::cmp::Ordering::Less => {
                    // Outdated nonce, ignore.
                    return Ok(());
                }
            }
        } else {
            // No session active, do check.
            debug_assert!(!self.is_coordinator());
            true
        };

        // Validate the new session request.
        if do_new_session_check
            && self.validate_new_session_init(
                initiator,
                their_context,
                their_nonce,
                their_ephmeral_pub,
                their_signature,
                their_package.clone(),
            )?
        {
            // A coordinator never reaches this point.
            debug_assert!(!self.is_coordinator());

            // New session accepted!
            return Ok(());
        }

        let self_is_coordinator = self.is_coordinator();

        let StageState::RoundOne {
            context,
            nonce,
            auth,
            in_round1_packages,
            out_round1_packages,
            ..
        } = &mut self.state
        else {
            if self.state.did_round_one_finalize() {
                // Send acknowledgments for previous rounds.
                self.queue.send_round1_ack(initiator, sender);
            }

            // Nothing left to do.
            return Ok(());
        };

        if in_round1_packages.contains_key(&initiator) {
            self.queue.send_round1_ack(initiator, sender);
            return Ok(());
        }

        let extracted_package = their_package.clone().extract(
            initiator,
            their_ephmeral_pub,
            their_signature,
            auth,
        )?;

        in_round1_packages.insert(initiator, extracted_package);
        self.queue.send_round1_ack(initiator, sender);

        if self_is_coordinator {
            // Forward the round1 package to all other members.
            for recipient in self.members.keys().copied() {
                if recipient == self.my_frost_id || recipient == sender {
                    continue;
                }

                // Track each outgoing package.
                let Some(entry) =
                    out_round1_packages.get_mut(&(initiator, recipient))
                else {
                    continue;
                };

                *entry = Some(OutEntryRoundOne {
                    package: their_package.clone(),
                    ephemeral_pub: their_ephmeral_pub,
                    signature: their_signature,
                    timer: None,
                    attempts: 0,
                });

                // Push outgoing payload to the queue.
                let msg = DkgPayload {
                    sender: self.my_frost_id,
                    recipient,
                    msg: DkgMessage::Round1 {
                        context: context.clone(),
                        nonce: *nonce,
                        initiator,
                        ephemeral_pub: their_ephmeral_pub,
                        signature: their_signature,
                        package: their_package.clone(),
                    },
                };

                self.queue.i.push_back(msg);
            }
        }

        Ok(())
    }
    fn on_dkg_msg_round2(
        &mut self,
        initiator: Initiator,
        target: Target,
        ciphernonce: u64,
        ciphertext: Vec<u8>,
        sender: frost::Identifier,
    ) -> Result<(), Error> {
        if !self.members.contains_key(&initiator.0) {
            return Ok(());
        }

        if initiator.0 == self.my_frost_id {
            return Ok(());
        }

        let self_is_coordinator = self.is_coordinator();

        let StageState::RoundTwo {
            pending,
            auth,
            in_round2_packages,
            out_round2_packages,
            ..
        } = &mut self.state
        else {
            if self.state.did_round_two_finalize() {
                // Send acknowledgments for previous rounds.
                self.queue.send_round2_ack(initiator, sender, target);
            }

            // Nothing left to do.
            return Ok(());
        };

        // If the target is us, we attempt to decrypt it and store the
        // plaintext version of it locally.
        if target.0 == self.my_frost_id {
            if in_round2_packages.contains_key(&initiator) {
                self.queue.send_round2_ack(initiator, sender, target);
                return Ok(());
            }

            let package =
                auth.validate_round2(initiator, ciphernonce, &ciphertext)?;

            // Insert the package.
            in_round2_packages.insert(initiator, package);
            self.queue.send_round2_ack(initiator, sender, target);

            // As a non-coordinator in the pending state, we start sending all
            // our round2 packages to the coordinator as soon as we receive the
            // first round2 package.
            if *pending {
                debug_assert!(!self_is_coordinator);

                for ((initiator, target), entry) in out_round2_packages.iter() {
                    let Some(entry) = entry else {
                        // Package not available.
                        continue;
                    };

                    debug_assert_eq!(initiator.0, self.my_frost_id);

                    // Push the pending, outgoing payload to the queue.
                    let msg = DkgPayload {
                        sender: self.my_frost_id,
                        recipient: self.coordinator,
                        msg: DkgMessage::Round2 {
                            initiator: *initiator,
                            target: *target,
                            nonce: entry.ciphernonce,
                            package: entry.ciphertext.clone(),
                        },
                    };

                    self.queue.i.push_back(msg);
                }

                *pending = false;
            }
        } else {
            // If we're the coordinator, then we forward this package to the
            // intended recipient.
            if !self_is_coordinator {
                // We're not the coordinator; just drop the package.
                return Ok(());
            }

            debug_assert!(!*pending);

            // Track the outgoing package.
            let Some(entry) = out_round2_packages.get_mut(&(initiator, target))
            else {
                return Ok(());
            };

            *entry = Some(OutEntryRoundTwo {
                ciphernonce,
                ciphertext: ciphertext.clone(),
                timer: None,
                attempts: 0,
            });

            self.queue.send_round2_ack(initiator, sender, target);

            // Push the outgoing payload to the queue.
            let msg = DkgPayload {
                sender: self.my_frost_id,
                recipient: target.0,
                msg: DkgMessage::Round2 {
                    initiator,
                    target,
                    nonce: ciphernonce,
                    package: ciphertext,
                },
            };

            self.queue.i.push_back(msg);
        }

        Ok(())
    }
    fn on_dkg_msg_round3(
        &mut self,
        initiator: Initiator,
        signing_commits: Option<frost::round1::SigningCommitments>,
        sender: frost::Identifier,
    ) -> Result<(), Error> {
        if !self.members.contains_key(&initiator.0) {
            return Ok(());
        }

        if initiator.0 == self.my_frost_id {
            return Ok(());
        }

        let self_is_coordinator = self.is_coordinator();

        let StageState::RoundThree {
            pending,
            in_round3_packages,
            out_round3_packages,
            ..
        } = &mut self.state
        else {
            if self.state.did_round_three_finalize() {
                // Send acknowledgments for previous rounds.
                self.queue.send_round3_ack(initiator, sender);
            }

            // Nothing left to do.
            return Ok(());
        };

        if in_round3_packages.contains_key(&initiator.0) {
            self.queue.send_round3_ack(initiator, sender);
            return Ok(());
        }

        // TODO: Explain this
        // TODO: Only the coordinator should do this(?)
        if let Some(signing_commits) = signing_commits {
            in_round3_packages.insert(initiator.0, signing_commits);
        }

        self.queue.send_round3_ack(initiator, sender);

        if *pending {
            debug_assert!(!self_is_coordinator);
            debug_assert_eq!(out_round3_packages.len(), 1);

            for (recipient, entry) in out_round3_packages.iter() {
                // Push the pending, outgoing payload to the queue.
                let msg = DkgPayload {
                    sender: self.my_frost_id,
                    recipient: *recipient,
                    msg: DkgMessage::Round3 {
                        initiator: Initiator(self.my_frost_id),
                        signing_commits: Some(entry.signing_commits),
                    },
                };

                self.queue.i.push_back(msg);
            }

            *pending = false;
        }

        Ok(())
    }
    fn on_dkg_msg_round4(
        &mut self,
        initiator: Initiator,
        their_signing_package: Option<frost::SigningPackage>,
        their_signature_share: frost::round2::SignatureShare,
        their_attestation_sig: secp256k1::ecdsa::Signature,
        sender: frost::Identifier,
    ) -> Result<(), Error> {
        if !self.members.contains_key(&initiator.0) {
            return Ok(());
        }

        if initiator.0 == self.my_frost_id {
            return Ok(());
        }

        let self_is_coordinator = self.is_coordinator();

        match &mut self.state {
            StageState::RoundFour {
                auth,
                signing_package,
                secret_package,
                public_key_package,
                in_round4_packages,
                out_round4_packages,
            } => {
                // TODO: Replace signing-package in this case? Or do some other validation?
                if in_round4_packages.contains_key(&initiator) {
                    self.queue.send_round4_ack(initiator, sender);
                    return Ok(());
                }

                auth.validate_signature_share(
                    initiator.0,
                    their_signature_share,
                    their_attestation_sig,
                )
                .unwrap();

                in_round4_packages.insert(
                    initiator,
                    (their_signature_share, their_attestation_sig),
                );
                self.queue.send_round4_ack(initiator, sender);

                if self_is_coordinator {
                    // Forward the round4 pacakge to all other members.
                    for recipient in self.members.keys().copied() {
                        if recipient == self.my_frost_id || recipient == sender
                        {
                            continue;
                        }

                        // Track each outgoing package.
                        let Some(entry) = out_round4_packages
                            .get_mut(&(initiator, recipient))
                        else {
                            continue;
                        };

                        *entry = Some(OutEntryRoundFour {
                            signature_share: their_signature_share.clone(),
                            attestation_sig: their_attestation_sig.clone(),
                            timer: None,
                            attempts: 0,
                        });

                        // Push outgoing payload to the queue.
                        let msg = DkgPayload {
                            sender: self.my_frost_id,
                            recipient,
                            msg: DkgMessage::Round4 {
                                initiator,
                                // TODO: Comment on this
                                signing_package: None,
                                signature_share: their_signature_share,
                                attestation_sig: their_attestation_sig,
                            },
                        };

                        self.queue.i.push_back(msg);
                    }
                }
            }
            StageState::AwaitingRoundFour {
                dkg_commit,
                secret_package,
                public_key_package,
                signing_nonces,
            } => {
                debug_assert!(!self_is_coordinator);

                let Some(signing_package) = their_signing_package else {
                    todo!()
                };

                if signing_package.message() != dkg_commit {
                    todo!()
                }

                let mut auth = AttestationManager::new(
                    self.multisig_id,
                    self.my_frost_id,
                    self.my_static_sec,
                    self.members.clone(),
                    signing_package.clone(),
                    public_key_package.clone(),
                )
                .unwrap();

                let our_signature_share = frost::round2::sign(
                    &signing_package,
                    &signing_nonces,
                    secret_package,
                )
                .unwrap();

                let our_attestation_sig =
                    auth.commit_signature_share(our_signature_share).unwrap();

                auth.validate_signature_share(
                    initiator.0,
                    their_signature_share,
                    their_attestation_sig,
                )
                .unwrap();
                self.queue.send_round4_ack(initiator, sender);

                let out_entry = OutEntryRoundFour {
                    signature_share: our_signature_share,
                    attestation_sig: our_attestation_sig,
                    timer: None,
                    attempts: 0,
                };

                let mut in_round4_packages = BTreeMap::new();

                in_round4_packages.insert(
                    Initiator(self.my_frost_id),
                    (our_signature_share, our_attestation_sig),
                );

                in_round4_packages.insert(
                    initiator,
                    (their_signature_share, their_attestation_sig),
                );

                let mut out_round4_packages = BTreeMap::new();
                out_round4_packages.insert(
                    (Initiator(self.my_frost_id), self.coordinator),
                    Some(out_entry),
                );

                let msg = DkgPayload {
                    sender: self.my_frost_id,
                    recipient: self.coordinator,
                    msg: DkgMessage::Round4 {
                        initiator: Initiator(self.my_frost_id),
                        // No need to send back the signing package to the
                        // coordinator.
                        signing_package: None,
                        signature_share: our_signature_share,
                        attestation_sig: our_attestation_sig,
                    },
                };

                self.queue.i.push_back(msg);

                self.state = StageState::RoundFour {
                    auth,
                    signing_package,
                    secret_package: secret_package.clone(),
                    public_key_package: public_key_package.clone(),
                    in_round4_packages,
                    out_round4_packages,
                };
            }
            _ => {
                if self.state.did_round_four_finalize() {
                    // Send acknowledgments for previous rounds.
                    self.queue.send_round4_ack(initiator, sender);
                }

                return Ok(());
            }
        }

        Ok(())
    }
    fn transition_stage2_checked(&mut self) -> Result<(), Error> {
        let StageState::RoundOne {
            auth,
            secret_package,
            in_round1_packages,
            out_round1_packages,
            ..
        } = &mut self.state
        else {
            // Ignore
            return Ok(());
        };

        // If we're still missing packages or there are un-acked outgoing
        // packages, return early. Do note that our round1 package is excluded.
        if in_round1_packages.len() != self.members.len() - 1
            || !out_round1_packages.is_empty()
        {
            return Ok(());
        }

        // Start transition.
        // TODO: `std::mem::take` is dangerous here -> Consider alternatives.
        // This applies to other places as well.
        let in_round1_packages = std::mem::take(in_round1_packages)
            .into_iter()
            .map(|(initiator, pkg)| (initiator.0, pkg))
            .collect();

        // Finalize DKG part2.
        let Ok((secret_package, dkg2_shares)) = frost::keys::dkg::part2(
            secret_package.clone(),
            &in_round1_packages,
        ) else {
            // On error, we abort the entire DKG process.
            //
            // TODO (lamafab): We should probably do some extra things here,
            // such as communicating this information to the coordinator/peers.
            self.state = StageState::Aborted;
            return Ok(());
        };

        // AUTHENTICATION: Proceed to the next round.
        let mut auth = auth.finalize()?;

        // Assigning the shares to the corresponding recipient and final
        // target.
        let mut out_round2_packages = BTreeMap::new();

        for (target, our_package) in dkg2_shares.clone() {
            let initiator = Initiator(self.my_frost_id);
            let target = Target(target);

            // AUTHENTICATION: Encrypt the package for the target individually.
            let (ciphertext, ciphernonce) =
                auth.commit_round2(&target, &our_package)?;

            let out_entry = OutEntryRoundTwo {
                ciphernonce,
                ciphertext: ciphertext.clone(),
                timer: None,
                attempts: 0,
            };

            // Track each outgoing package.
            out_round2_packages.insert((initiator, target), Some(out_entry));

            if self.is_coordinator() {
                // Push each outgoing payload to the queue.
                let msg = DkgPayload {
                    sender: self.my_frost_id,
                    recipient: target.0,
                    msg: DkgMessage::Round2 {
                        initiator,
                        target,
                        nonce: ciphernonce,
                        package: ciphertext,
                    },
                };

                self.queue.i.push_back(msg);
            }
        }

        // Non-coordinators will wait until they receive the first
        // round2 package before they start sending theirs.
        let pending = !self.is_coordinator();

        if self.is_coordinator() {
            // As the coordinator, we prepare all additional outgoing/forwarded
            // round2 package entries that we need to have acknowledged.
            //
            // For example; with three participants Alice (us), Bob, and Eve, we construct:
            // * Bob -> Eve (forwarded)
            // * Eve -> Bob (forwarded)
            for initiator in self.members.keys().copied() {
                // Skip ourself, we set those entries before.
                if initiator == self.my_frost_id {
                    continue;
                }

                for target in self.members.keys().copied() {
                    // Skip ourself.
                    if target == self.my_frost_id {
                        continue;
                    }

                    if initiator == target {
                        continue;
                    }

                    // Create an entry with an empty package; it will be updated
                    // when it's received.
                    out_round2_packages
                        .insert((Initiator(initiator), Target(target)), None);
                }
            }
        }

        self.state = StageState::RoundTwo {
            auth,
            pending,
            secret_package,
            in_round1_packages,
            in_round2_packages: BTreeMap::new(),
            out_round2_packages,
        };

        Ok(())
    }
    fn transition_stage3_checked(&mut self) -> Result<(), Error> {
        let StageState::RoundTwo {
            pending,
            auth,
            secret_package,
            in_round1_packages,
            in_round2_packages,
            out_round2_packages,
        } = &mut self.state
        else {
            // Ignore
            return Ok(());
        };

        debug_assert_eq!(in_round1_packages.len(), self.members.len() - 1);

        // If we're still missing packages or there are un-acked outgoing
        // packages, return early. Do note that our round2 package is excluded.
        if in_round2_packages.len() != self.members.len() - 1
            || !out_round2_packages.is_empty()
        {
            return Ok(());
        }

        debug_assert!(!*pending);

        // Start transition.
        let in_round1_packages = std::mem::take(in_round1_packages);
        let in_round2_packages = std::mem::take(in_round2_packages)
            .into_iter()
            .map(|(initiator, pkg)| (initiator.0, pkg))
            .collect();

        // Finalize DKG part3
        let Ok((secret_package, public_key_package)) = frost::keys::dkg::part3(
            secret_package,
            &in_round1_packages,
            &in_round2_packages,
        ) else {
            // On error, we abort the entire DKG process.
            //
            // TODO (lamafab): We should probably do some extra things here,
            // such as communicating this information to the coordinator/peers.
            return Ok(());
        };

        // TODO: Update doc
        // AUTHENTICATION: Proceed to the next round, then sign and seal the
        // package.
        let dkg_commit = auth.finalize()?;

        let (signing_nonces, signing_commits) = frost::round1::commit(
            secret_package.signing_share(),
            &mut thread_rng(),
        );

        let out_entry =
            OutEntryRoundThree { signing_commits, timer: None, attempts: 0 };

        let mut in_round3_packages = BTreeMap::new();
        let mut out_round3_packages = BTreeMap::new();
        let pending = !self.is_coordinator();

        if self.is_coordinator() {
            in_round3_packages.insert(self.my_frost_id, signing_commits);

            for recipient in self.members.keys().copied() {
                // Skip ourself.
                if recipient == self.my_frost_id {
                    continue;
                }

                out_round3_packages.insert(recipient, out_entry.clone());

                let msg = DkgPayload {
                    sender: self.my_frost_id,
                    recipient,
                    msg: DkgMessage::Round3 {
                        initiator: Initiator(self.my_frost_id),
                        // TODO: Comment on this.
                        signing_commits: None,
                    },
                };

                self.queue.i.push_back(msg);
            }
        } else {
            // Non-coordinators only have one outgoing package to send (to the
            // coordinator).
            out_round3_packages.insert(self.coordinator, out_entry);
        }

        // TODO: Should this be unified?
        self.state = StageState::RoundThree {
            pending,
            dkg_commit,
            public_key_package,
            secret_package,
            signing_nonces,
            in_round3_packages,
            out_round3_packages,
        };

        Ok(())
    }
    fn transition_stage4_checked(&mut self) -> Result<(), Error> {
        let self_is_coordinator = self.is_coordinator();

        let StageState::RoundThree {
            pending,
            dkg_commit,
            secret_package,
            public_key_package,
            signing_nonces,
            in_round3_packages,
            out_round3_packages,
        } = &mut self.state
        else {
            // Ignore
            return Ok(());
        };

        // If we're still missing packages or there are un-acked outgoing
        // packages, return early. Do note that only the coordinator must
        // collect the messages (signing commitments) for this step, including
        // its own.
        if self_is_coordinator && in_round3_packages.len() != self.members.len()
        {
            return Ok(());
        }

        if !out_round3_packages.is_empty() {
            return Ok(());
        }

        // The coordinator kicks-off the round immediately, while
        // non-coordinators remain in a waiting state until the coordinators'
        // payloads have been received.
        if self_is_coordinator {
            // Construct the signing package: we use the deterministically
            // generated DKG commitment as produced by
            // [`SecureChannelManager::finalize`](SecureChannelManager::finalize)
            // as the package message, which each non-coordinator validates and
            // enforces locally.
            let in_round3_packages = std::mem::take(in_round3_packages);
            let signing_package =
                frost::SigningPackage::new(in_round3_packages, dkg_commit);

            // Produce our signature share.
            let signature_share = frost::round2::sign(
                &signing_package,
                &signing_nonces,
                secret_package,
            )
            .unwrap();

            // AUTHENTICATION: Setup attestation manager.
            let mut auth = AttestationManager::new(
                self.multisig_id,
                self.my_frost_id,
                self.my_static_sec,
                self.members.clone(),
                signing_package.clone(),
                public_key_package.clone(),
            )
            .unwrap();

            // Commit our own signature share and construct our attestation
            // signature--both of which are part of the round4 package payload.
            let attestation_sig =
                auth.commit_signature_share(signature_share).unwrap();

            // Track our own round4 package.
            let mut in_round4_packages = BTreeMap::new();
            in_round4_packages.insert(
                Initiator(self.my_frost_id),
                (signature_share, attestation_sig),
            );

            let mut out_round4_packages = BTreeMap::new();
            let out_entry = OutEntryRoundFour {
                signature_share,
                attestation_sig,
                timer: None,
                attempts: 0,
            };

            // Prepare all outgoing round4 package entries that we need to have
            // acknowledged, including forwarded messages.
            //
            // For example; with three participants Alice (us), Bob, and Eve, we construct:
            // * Alice -> Bob
            // * Alice -> Eve
            // * Bob -> Eve (forwarded)
            // * Eve -> Bob (forwarded)
            for initiator in self.members.keys().copied() {
                for recipient in self.members.keys().copied() {
                    // Skip ourself.
                    if recipient == self.my_frost_id {
                        continue;
                    }

                    if initiator == recipient {
                        continue;
                    }

                    // Only set our packages; forwarded packages are set once
                    // they're received, of course.
                    let out_entry = if initiator == self.my_frost_id {
                        Some(out_entry.clone())
                    } else {
                        None
                    };

                    out_round4_packages
                        .insert((Initiator(initiator), recipient), out_entry);

                    if initiator != self.my_frost_id {
                        // Skip sending unless it's us.
                        continue;
                    }

                    let msg = DkgPayload {
                        sender: self.my_frost_id,
                        recipient,
                        msg: DkgMessage::Round4 {
                            initiator: Initiator(self.my_frost_id),
                            signing_package: Some(signing_package.clone()),
                            signature_share,
                            attestation_sig,
                        },
                    };

                    self.queue.i.push_back(msg);
                }
            }

            self.state = StageState::RoundFour {
                auth,
                signing_package,
                secret_package: secret_package.clone(),
                public_key_package: public_key_package.clone(),
                in_round4_packages,
                out_round4_packages,
            };
        } else {
            // TODO: This triggers during a specific resend test.
            //debug_assert!(in_round3_packages.is_empty());

            self.state = StageState::AwaitingRoundFour {
                dkg_commit: *dkg_commit,
                secret_package: secret_package.clone(),
                public_key_package: public_key_package.clone(),
                signing_nonces: signing_nonces.clone(),
            };
        }

        Ok(())
    }
    fn transition_final_checked(&mut self) -> Result<(), Error> {
        let StageState::RoundFour {
            auth,
            signing_package,
            secret_package,
            public_key_package,
            in_round4_packages,
            out_round4_packages,
        } = &mut self.state
        else {
            // Ignore
            return Ok(());
        };

        // If we're still missing packages or there are un-acked outgoing
        // packages, return early. Do note that our own round4 package is
        // included here.
        if in_round4_packages.len() != self.members.len()
            || !out_round4_packages.is_empty()
        {
            return Ok(());
        }

        // AUTHENTICATION: The attestation manager must be able to compute the
        // aggregated signature with each members signature share and verify it
        // against the aggregated public key. On success, this confirms that the
        // multisig setup works reliably and correctly.
        let aggregated_sig = auth.finalize().unwrap();

        let attestations = std::mem::take(in_round4_packages)
            .into_iter()
            .map(|(k, v)| (k.0, v))
            .collect();

        // Mark the DKG process as finalized.
        self.state = StageState::Finalized {
            secret_package: secret_package.clone(),
            public_key_package: public_key_package.clone(),
            signing_package: signing_package.clone(),
            aggregated_sig,
            attestations,
        };

        // Reset session parameters.
        self.session_nonce = None;
        self.session_activated = None;

        Ok(())
    }
}

/*
if self.is_coordinator() {
    // Prepare all outgoing round3 package entries that we need to have
    // acknowledged, including forwarded messages.
    //
    // For example; with three participants Alice (us), Bob, and Eve, we construct:
    // * Alice -> Bob
    // * Alice -> Eve
    // * Bob -> Eve (forwarded)
    // * Eve -> Bob (forwarded)
    for initiator in self.members.keys().copied() {
        for recipient in self.members.keys().copied() {
            // Skip ourself.
            if recipient == self.my_frost_id {
                continue;
            }

            if initiator == recipient {
                continue;
            }

            // Only set our packages; forwarded packages are set once
            // they're received, of course.
            let out_entry = if initiator == self.my_frost_id {
                Some(out_entry.clone())
            } else {
                None
            };

            out_round3_packages
                .insert((Initiator(initiator), recipient), out_entry);

            if initiator != self.my_frost_id {
                // Skip sending unless it's us.
                continue;
            }

            let msg = DkgPayload {
                sender: self.my_frost_id,
                recipient,
                msg: DkgMessage::Round3 {
                    initiator: Initiator(self.my_frost_id),
                    signature: our_sealed_signature.clone(),
                },
            };

            self.queue.i.push_back(msg);
        }
    }
} else {
    // Non-coordinators only have one outgoing package to send (to the
    // coordinator).
    out_round3_packages.insert(
        (Initiator(self.my_frost_id), self.coordinator),
        Some(out_entry),
    );
}
*/
