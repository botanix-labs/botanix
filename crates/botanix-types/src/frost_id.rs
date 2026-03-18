use frost_secp256k1_tr as frost;

/// Convenience wrapper for the [`frost::Identifier`] that implements various
/// traits.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct FrostId(frost::Identifier);

impl std::ops::Deref for FrostId {
    type Target = frost::Identifier;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<u16> for FrostId {
    fn from(value: u16) -> Self {
        let id = frost::Identifier::derive(&value.to_le_bytes())
            .expect("frost id must be derivable");
        Self(id)
    }
}

impl From<frost::Identifier> for FrostId {
    fn from(value: frost::Identifier) -> Self {
        Self(value)
    }
}

impl From<secp256k1::PublicKey> for FrostId {
    fn from(value: secp256k1::PublicKey) -> Self {
        Self::from(&value)
    }
}

impl From<&secp256k1::PublicKey> for FrostId {
    fn from(value: &secp256k1::PublicKey) -> Self {
        let id = frost::Identifier::derive(&value.serialize())
            .expect("frost id must be derivable");
        Self(id)
    }
}

impl std::fmt::Display for FrostId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = hex::encode(self.0.serialize());
        write!(f, "{s}")
    }
}
