//! MultisigId type for identifying multisig federations.

use serde::{Deserialize, Serialize};
use std::fmt;

#[cfg(feature = "compact")]
use bytes::BufMut;
#[cfg(feature = "compact")]
use reth_codecs::Compact;

/// Wrapper type for multisig IDs.
///
/// This type provides a type-safe wrapper around the raw `u32` multisig identifier,
/// used to distinguish between different multisig federations in the Botanix network.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct MultisigId(u32);

impl MultisigId {
    /// The legacy multisig ID constant (pre-dynafed).
    pub const LEGACY: Self = Self(0);

    /// Create a new MultisigId.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the inner value as u32.
    pub const fn as_u32(&self) -> u32 {
        self.0
    }
}

/// The legacy multisig ID constant for backwards compatibility.
pub const LEGACY_MULTISIG_ID: MultisigId = MultisigId::LEGACY;

/// Test constant that aliases LEGACY_MULTISIG_ID for use in tests.
pub const TEST_LEGACY_MULTISIG_ID: MultisigId = LEGACY_MULTISIG_ID;

/// Default function for serde to use LEGACY_MULTISIG_ID as the default value.
pub const fn default_multisig_id() -> MultisigId {
    MultisigId::LEGACY
}

impl From<u32> for MultisigId {
    fn from(id: u32) -> Self {
        Self::new(id)
    }
}

impl From<MultisigId> for u32 {
    fn from(id: MultisigId) -> Self {
        id.as_u32()
    }
}

impl fmt::Display for MultisigId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Deref for MultisigId {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(feature = "compact")]
impl Compact for MultisigId {
    fn to_compact<B>(&self, buf: &mut B) -> usize
    where
        B: BufMut + AsMut<[u8]>,
    {
        // Convert to u64 for Compact encoding (u32 doesn't implement Compact)
        (self.0 as u64).to_compact(buf)
    }

    fn from_compact(buf: &[u8], len: usize) -> (Self, &[u8]) {
        // Decode as u64 then convert back to u32
        let (val, remaining) = u64::from_compact(buf, len);
        (Self(val as u32), remaining)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multisig_id_default() {
        assert_eq!(MultisigId::default(), MultisigId::LEGACY);
        assert_eq!(MultisigId::default().as_u32(), 0);
    }

    #[test]
    fn test_multisig_id_from_u32() {
        let id: MultisigId = 42u32.into();
        assert_eq!(id.as_u32(), 42);
    }

    #[test]
    fn test_multisig_id_into_u32() {
        let id = MultisigId::new(42);
        let val: u32 = id.into();
        assert_eq!(val, 42);
    }

    #[test]
    fn test_multisig_id_display() {
        let id = MultisigId::new(123);
        assert_eq!(format!("{}", id), "123");
    }

    #[test]
    fn test_multisig_id_deref() {
        let id = MultisigId::new(42);
        assert_eq!(*id, 42u32);
    }

    #[test]
    fn test_legacy_constants() {
        assert_eq!(LEGACY_MULTISIG_ID, MultisigId::LEGACY);
        assert_eq!(TEST_LEGACY_MULTISIG_ID, LEGACY_MULTISIG_ID);
        assert_eq!(default_multisig_id(), MultisigId::LEGACY);
    }

    #[test]
    fn test_serde_roundtrip() {
        let id = MultisigId::new(42);
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: MultisigId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }
}
