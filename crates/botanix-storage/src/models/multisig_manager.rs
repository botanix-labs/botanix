use std::collections::BTreeMap;

use botanix_types::MultisigId;
use frost_secp256k1_tr::{self as frost, keys::PublicKeyPackage};
use serde::{Deserialize, Serialize};

/// A multisig record stored in the database.
///
/// Used for enumerating all known multisigs regardless of lifecycle phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MultisigRecord {
    /// A multisig that has been registered via local config but not yet
    /// attested via DKG.
    Staged {
        /// The staged multisig entry.
        entry: StagedMultisigEntry,
    },
    /// A multisig that has completed DKG attestation and holds a valid
    /// [`PublicKeyPackage`].
    Attested {
        /// The attested multisig entry.
        entry: AttestedMultisigEntry,
    },
}

impl From<StagedMultisigEntry> for MultisigRecord {
    fn from(entry: StagedMultisigEntry) -> Self {
        MultisigRecord::Staged { entry }
    }
}

impl From<AttestedMultisigEntry> for MultisigRecord {
    fn from(entry: AttestedMultisigEntry) -> Self {
        MultisigRecord::Attested { entry }
    }
}

/// Lifecycle status of an attested multisig.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MultisigStatus {
    /// Receives new pegins; exactly one multisig holds this status.
    Funding,
    /// Superseded by a newer funding multisig; still active for signing.
    Degrading,
    /// Marked for removal by the coordinator; no longer in the active set.
    Sunsetting,
}

/// A multisig that has been registered for an upcoming DKG ceremony but has not
/// yet been attested. At most one staged multisig may exist at any time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedMultisigEntry {
    /// Unique identifier for this multisig.
    pub multisig_id: MultisigId,
    /// The FROST identifier of the designated coordinator.
    pub coordinator: frost::Identifier,
    /// Mapping of FROST identifiers to secp256k1 public keys for all federation
    /// members.
    pub fed_members: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
}

/// A fully attested multisig that has completed DKG and holds a valid
/// [`PublicKeyPackage`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestedMultisigEntry {
    /// Unique identifier for this multisig.
    pub multisig_id: MultisigId,
    /// The FROST identifier of the designated coordinator.
    pub coordinator: frost::Identifier,
    /// Mapping of FROST identifiers to secp256k1 public keys for all federation
    /// members.
    pub fed_members: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
    /// The FROST public key package containing the group verifying key and
    /// member shares.
    pub public_key_package: PublicKeyPackage,
    /// Current lifecycle status. See [`MultisigStatus`] for the state machine.
    pub status: MultisigStatus,
}

impl AttestedMultisigEntry {
    /// Returns the aggregated secp256k1 public key for this multisig.
    ///
    /// This extracts the group verifying key from the FROST
    /// [`PublicKeyPackage`] and converts it to a [`secp256k1::PublicKey`]. The
    /// resulting key is used for Bitcoin script construction such as Taproot
    /// addresses and pegin validation.
    pub fn aggregate_public_key(&self) -> secp256k1::PublicKey {
        let bytes = self
            .public_key_package
            .verifying_key()
            .serialize()
            .expect("verifying key must be serializable");

        secp256k1::PublicKey::from_slice(&bytes)
            .expect("verifying key must be valid")
    }
}
