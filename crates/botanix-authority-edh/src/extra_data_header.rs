use bitcoin::{
    consensus::encode::{self, Decodable, Encodable},
    hashes::Hash,
};
use revm_primitives::Address;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, io};
use thiserror::Error;

use crate::nums_secp256k1_pk;

/// Deprecated legacy version that supports only one aggregated public key
pub const EXTRA_HEADER_VERSION_V0: u32 = 0;
/// Version that supports multiple aggregated public keys
pub const EXTRA_HEADER_VERSION_V1: u32 = 1;
/// The version of the chain
pub const CHAIN_VERSION: u32 = 0;

/// Metadata fields that are included in the extra data header of botanix blocks
/// Federation members sign this data attesting to a new block and the set of authority signers
/// A block producer will sign `Hash(block_hash || extra_data_version || authority_signers ||
/// bitcoin_block_hash ... )` This sighash excludes the authority signature field.
/// Use `encode_into_without_signature` to serialize the extradata header with out the signature
/// field Note: the order of the struct properties is important for serialization/deserialization
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExtraDataHeader {
    /// The version of the extra data header
    pub version: u32,
    /// Chain version that determines the valid chain
    /// this is a distinct field from chain id
    pub chain_version: u32,
    /// The hash of the bitcoin block that is sufficiently deep to prove pegins
    pub bitcoin_block_hash: bitcoin::hash_types::BlockHash,
    /// Aggregated public keys
    pub aggregated_public_keys: HashSet<secp256k1::PublicKey>,
    /// Block producer address
    pub block_fee_recipient_address: Address,
}

impl Default for ExtraDataHeader {
    // Note: default should never be used outside of tests
    fn default() -> Self {
        let mut aggregated_public_keys = HashSet::new();
        aggregated_public_keys.insert(nums_secp256k1_pk());
        Self {
            version: EXTRA_HEADER_VERSION_V1,
            chain_version: CHAIN_VERSION,
            bitcoin_block_hash: bitcoin::hash_types::BlockHash::all_zeros(),
            aggregated_public_keys,
            block_fee_recipient_address: Address::ZERO,
        }
    }
}

/// Errors that can occur when deserializing the extra data header
#[derive(Debug, Error)]
pub enum ExtraDataHeaderDeserializeError {
    #[error("I/O error")]
    /// I/O error
    Io(#[from] bitcoin::io::Error),
    #[error("invalid data format")]
    /// Invalid data format
    Decoding(#[from] encode::Error),
    #[error("invalid version")]
    /// Invalid EDH version
    InvalidVersion,
}

/// Errors that can occur when serializing the extra data header
#[derive(Debug, Error)]
pub enum ExtraDataHeaderSerializeError {
    #[error("Invalid format: {0}")]
    /// Invalid EDH format
    InvalidFormat(&'static str),
}

impl ExtraDataHeader {
    /// Create a new extra data header
    pub const fn new(
        version: u32,
        // Chain version that determines the valid chain
        chain_version: u32,
        // The hash of the bitcoin block that is sufficiently deep to prove pegins
        bitcoin_block_hash: bitcoin::hash_types::BlockHash,
        // Aggregated public keys
        aggregated_public_keys: HashSet<secp256k1::PublicKey>,
        // Block producer address
        block_fee_recipient_address: Address,
    ) -> Self {
        Self {
            version,
            chain_version,
            bitcoin_block_hash,
            aggregated_public_keys,
            block_fee_recipient_address,
        }
    }

    /// Serialize the extra data header without the signature
    /// Always serializes in V1 format (count + multiple public keys)
    pub fn encode_into_without_signature(
        &self,
        writer: &mut impl bitcoin::io::Write,
    ) -> Result<(), io::Error> {
        self.version.consensus_encode(writer)?;
        self.chain_version.consensus_encode(writer)?;
        self.bitcoin_block_hash.consensus_encode(writer)?;

        // V1 format: count + multiple public keys
        let num_keys = self.aggregated_public_keys.len() as u16;
        num_keys.consensus_encode(writer)?;

        // Serialize and sort keys for deterministic serialization
        let mut serialized_keys: Vec<[u8; 33]> =
            self.aggregated_public_keys.iter().map(|k| k.serialize()).collect();
        serialized_keys.sort();

        for key_bytes in serialized_keys {
            key_bytes.consensus_encode(writer)?;
        }

        let block_producer_address_bytes =
            self.block_fee_recipient_address.0 .0;
        let _ = writer.write(&block_producer_address_bytes)?;

        Ok(())
    }

    /// Serialize the extra data header into the writer.
    pub fn encode_into(
        &self,
        writer: &mut impl bitcoin::io::Write,
    ) -> Result<(), io::Error> {
        self.encode_into_without_signature(writer)?;
        Ok(())
    }

    /// Serialize the extra data header
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.encode_into(&mut buf).expect("buffers produce no io errors");
        buf
    }

    /// Deserialize the extra data header
    pub fn deserialize(
        reader: &mut impl bitcoin::io::Read,
    ) -> Result<Self, ExtraDataHeaderDeserializeError> {
        let version = u32::consensus_decode(reader)?;
        let chain_version = u32::consensus_decode(reader)?;
        let bitcoin_block_hash = Decodable::consensus_decode(reader)?;

        // Deserialize specific versions of edh based on the version
        let aggregated_public_keys = match version {
            EXTRA_HEADER_VERSION_V0 => {
                // V0 format: single public key (backward compatibility)
                let pk_bytes = <[u8; 33]>::consensus_decode(reader)?;
                let aggregated_public_key = secp256k1::PublicKey::from_slice(&pk_bytes)
                    .map_err(|e| {
                        println!("Error: {:?}", e);
                        encode::Error::ParseFailed("malformed aggregate public key")
                    })?;

                let mut keys = HashSet::new();
                keys.insert(aggregated_public_key);
                keys
            }
            EXTRA_HEADER_VERSION_V1 => {
                // V1 format: count + multiple public keys
                let num_keys = u16::consensus_decode(reader)?;
                let mut keys = HashSet::new();

                for _ in 0..num_keys {
                    let pk_bytes = <[u8; 33]>::consensus_decode(reader)?;
                    let public_key = secp256k1::PublicKey::from_slice(&pk_bytes)
                        .map_err(|e| {
                            println!("Error: {:?}", e);
                            encode::Error::ParseFailed("malformed aggregate public key")
                        })?;
                    keys.insert(public_key);
                }

                keys
            }
            _ => {
                return Err(ExtraDataHeaderDeserializeError::InvalidVersion);
            }
        };

        let mut block_fee_recipient_address_bytes: [u8; 20] = [0; 20];
        reader.read_exact(&mut block_fee_recipient_address_bytes)?;
        let block_fee_recipient_address =
            Address::from_slice(&block_fee_recipient_address_bytes);

        Ok(Self {
            version,
            chain_version,
            bitcoin_block_hash,
            aggregated_public_keys,
            block_fee_recipient_address,
        })
    }

    /// returns the edh max size
    pub fn edh_max_size() -> usize {
        let edh = Self::default();
        edh.serialize().len()
    }

    /// returns the edh size
    pub fn edh_size(&self) -> usize {
        self.serialize().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::BlockHash;
    use bitcoin::consensus::encode::Encodable;
    use rand::rngs::OsRng;
    use revm_primitives::hex;
    use secp256k1::Secp256k1;

    // Test case for creating a new ExtraDataHeader
    #[test]
    fn test_create_new_header() {
        let mainchain = BlockHash::hash(&[1, 2, 3]);
        let mut aggregated_public_keys = HashSet::new();
        aggregated_public_keys.insert(nums_secp256k1_pk());

        let header = ExtraDataHeader::new(
            EXTRA_HEADER_VERSION_V1,
            CHAIN_VERSION,
            mainchain,
            aggregated_public_keys.clone(),
            Address::ZERO,
        );
        assert_eq!(header.version, EXTRA_HEADER_VERSION_V1);
        assert_eq!(header.chain_version, CHAIN_VERSION);
        assert_eq!(header.bitcoin_block_hash, mainchain);
        assert_eq!(header.aggregated_public_keys, aggregated_public_keys);
    }

    // Test case for edh max size
    #[test]
    fn check_max_edh_size() {
        assert!(ExtraDataHeader::edh_max_size() == 93);
    }

    // Test case for serializing without a signature
    #[test]
    fn serialize_without_signature() {
        let mut authority_signers = vec![];
        // Generate some pks
        let secp = Secp256k1::new();
        let (_, public_key) = secp.generate_keypair(&mut OsRng);
        authority_signers.push(public_key);
        let address = Address::random();

        let mut aggregated_public_keys = HashSet::new();
        aggregated_public_keys.insert(nums_secp256k1_pk());

        let header = ExtraDataHeader::new(
            EXTRA_HEADER_VERSION_V1,
            CHAIN_VERSION,
            BlockHash::hash(&[1]),
            aggregated_public_keys,
            address,
        );
        let mut buf: Vec<u8> = vec![];
        header.encode_into_without_signature(&mut buf).unwrap();
        // serialize the same header
        let serialized = ExtraDataHeader::deserialize(&mut buf.as_slice())
            .expect("Deserialization");
        assert_eq!(serialized, header);
    }

    #[test]
    fn create_botanix_testnet_header() {
        let pk1 = secp256k1::PublicKey::from_slice(
            hex::decode("039bef292b80427d355cecb89eda8a50a7d2196a93d73dade5a0c4a07cd334815d")
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        let pk2 = secp256k1::PublicKey::from_slice(
            hex::decode("02bdc272b244f717604fffe659d2d98205d1e6764fdf453d1631f42c2db4d8d710")
                .unwrap()
                .as_slice(),
        )
        .unwrap();

        let mut aggregated_public_keys = HashSet::new();
        aggregated_public_keys.insert(pk1);
        aggregated_public_keys.insert(pk2);

        let extra_data_header = ExtraDataHeader::new(
            EXTRA_HEADER_VERSION_V1,
            CHAIN_VERSION,
            BlockHash::hash(&[1]),
            aggregated_public_keys,
            Address::ZERO,
        );

        println!(
            "serialized header: {}",
            hex::encode(extra_data_header.serialize())
        );
    }

    // Test EDH_V1 format with multiple keys
    #[test]
    fn test_v1_multiple_keys_roundtrip() {
        let secp = Secp256k1::new();
        let mut aggregated_public_keys = HashSet::new();

        // Generate 3 different public keys
        for _ in 0..3 {
            let (_, public_key) = secp.generate_keypair(&mut OsRng);
            aggregated_public_keys.insert(public_key);
        }

        let header = ExtraDataHeader::new(
            EXTRA_HEADER_VERSION_V1,
            CHAIN_VERSION,
            BlockHash::hash(&[1, 2, 3]),
            aggregated_public_keys.clone(),
            Address::random(),
        );

        // Serialize
        let mut buf = Vec::new();
        header.encode_into(&mut buf).unwrap();

        // Deserialize
        let deserialized = ExtraDataHeader::deserialize(&mut buf.as_slice())
            .expect("Deserialization failed");

        // Verify
        assert_eq!(deserialized.version, EXTRA_HEADER_VERSION_V1);
        assert_eq!(deserialized.aggregated_public_keys.len(), 3);
        assert_eq!(deserialized.aggregated_public_keys, aggregated_public_keys);
        assert_eq!(deserialized, header);
    }

    // Test EDH_V1 format deterministic serialization
    #[test]
    fn test_v1_deterministic_serialization() {
        let secp = Secp256k1::new();
        let mut keys = Vec::new();

        // Generate keys
        for _ in 0..3 {
            let (_, public_key) = secp.generate_keypair(&mut OsRng);
            keys.push(public_key);
        }

        // Create two headers with same keys in different insertion order
        let mut keys_set1 = HashSet::new();
        keys_set1.insert(keys[0]);
        keys_set1.insert(keys[1]);
        keys_set1.insert(keys[2]);

        let mut keys_set2 = HashSet::new();
        keys_set2.insert(keys[2]);
        keys_set2.insert(keys[0]);
        keys_set2.insert(keys[1]);

        let header1 = ExtraDataHeader::new(
            EXTRA_HEADER_VERSION_V1,
            CHAIN_VERSION,
            BlockHash::hash(&[1]),
            keys_set1,
            Address::ZERO,
        );

        let header2 = ExtraDataHeader::new(
            EXTRA_HEADER_VERSION_V1,
            CHAIN_VERSION,
            BlockHash::hash(&[1]),
            keys_set2,
            Address::ZERO,
        );

        // Both should serialize to the same bytes (deterministic)
        let serialized1 = header1.serialize();
        let serialized2 = header2.serialize();
        assert_eq!(serialized1, serialized2);
    }

    // Test EDH_V0 backward compatibility - deserialize V0 format
    #[test]
    fn test_v0_backward_compatibility() {
        

        let mut buf = Vec::new();
        let version_v0 = EXTRA_HEADER_VERSION_V0;
        let chain_version = CHAIN_VERSION;
        let bitcoin_block_hash = BlockHash::hash(&[1, 2, 3]);
        let public_key = nums_secp256k1_pk();
        let address = Address::ZERO;

        // Manually serialize V0 format (version, chain_version, block_hash, single key, address)
        version_v0.consensus_encode(&mut buf).unwrap();
        chain_version.consensus_encode(&mut buf).unwrap();
        bitcoin_block_hash.consensus_encode(&mut buf).unwrap();
        public_key.serialize().consensus_encode(&mut buf).unwrap();
        buf.extend_from_slice(&address.0.0);

        // Deserialize as V0
        let deserialized = ExtraDataHeader::deserialize(&mut buf.as_slice())
            .expect("V0 deserialization failed");

        // Verify it was parsed as V0 with single key converted to HashSet
        assert_eq!(deserialized.version, EXTRA_HEADER_VERSION_V0);
        assert_eq!(deserialized.aggregated_public_keys.len(), 1);
        assert!(deserialized.aggregated_public_keys.contains(&public_key));
        assert_eq!(deserialized.bitcoin_block_hash, bitcoin_block_hash);
        assert_eq!(deserialized.block_fee_recipient_address, address);
    }

    #[test]
    fn test_v1_single_key() {
        let mut aggregated_public_keys = HashSet::new();
        aggregated_public_keys.insert(nums_secp256k1_pk());

        let header = ExtraDataHeader::new(
            EXTRA_HEADER_VERSION_V1,
            CHAIN_VERSION,
            BlockHash::hash(&[5]),
            aggregated_public_keys,
            Address::ZERO,
        );

        // Serialize and deserialize
        let serialized = header.serialize();
        let deserialized = ExtraDataHeader::deserialize(&mut serialized.as_slice())
            .expect("Deserialization failed");

        assert_eq!(deserialized, header);
        assert_eq!(deserialized.aggregated_public_keys.len(), 1);
    }

    // Test invalid version
    #[test]
    fn test_invalid_version() {
        use bitcoin::consensus::encode::Encodable;

        let mut buf = Vec::new();
        let invalid_version = 999u32;
        let chain_version = CHAIN_VERSION;
        let bitcoin_block_hash = BlockHash::hash(&[1]);

        // Write invalid version
        invalid_version.consensus_encode(&mut buf).unwrap();
        chain_version.consensus_encode(&mut buf).unwrap();
        bitcoin_block_hash.consensus_encode(&mut buf).unwrap();

        // Try to deserialize - should fail with InvalidVersion
        let result = ExtraDataHeader::deserialize(&mut buf.as_slice());
        assert!(result.is_err());
        match result {
            Err(ExtraDataHeaderDeserializeError::InvalidVersion) => {
                // Expected error
            }
            _ => panic!("Expected InvalidVersion error"),
        }
    }

}
