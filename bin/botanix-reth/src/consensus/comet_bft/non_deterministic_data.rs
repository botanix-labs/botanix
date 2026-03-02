//! Non-deterministic data (NDD) used for extend cometbft blocks with botanix specific data.

use alloy_primitives::Address;
use bitcoin::consensus::encode::{self, Decodable, Encodable};
use botanix_activation_manager::NetworkUpgradePayload;
use botanix_storage::models::{
    MajorVersion, MinorVersion, RuntimeVersion, Vote,
};
use std::io::{self, Write};
use thiserror::Error;

use crate::consensus::multisig_manager;

/// Errors that can occur when deserializing NonDeterministicData
#[derive(Debug, Error)]
#[non_exhaustive]
pub(crate) enum NonDeterministicDataDeserializeError {
    #[error("I/O error")]
    /// I/O error
    Io(#[from] io::Error),
    #[error("invalid data format")]
    /// Invalid data format
    Decoding(#[from] encode::Error),
    #[error("invalid version")]
    /// Invalid NonDeterministicData, version
    InvalidVersion,
    /// Invalid inclusion indicator, equivalent to Some/None
    #[error("invalid inclusion indicator")]
    InclusionIndicator,
    /// // Invalid network upgrade vote
    #[error("invalid network upgrade vote")]
    NetworkUpgradePayloadVote,
    /// // Invalid multisig manager message
    #[error("invalid multisig manager message")]
    MultisigManagerMessage,
}

/// The implied Botanix runtime version at mainnet launch, created
/// retroactively.
pub(crate) const RUNTIME_VERSION_GENESIS: RuntimeVersion =
    RuntimeVersion::new(0, 1);

/// Does not require `block_fee_recipient_address` to be present in NDD
/// Only supported on testnet for historical syncing purposes
const VERSION_0: u16 = 0;
/// Requires `block_fee_recipient_address` to be present in NDD
/// Supported on testnet and mainnet
const VERSION_1: u16 = 1;
/// Allows for custom runtime version indicators and an optional network upgrade
/// payload.
const VERSION_2: u16 = 2;
/// TODO
const VERSION_3: u16 = 3;

/// Type that encapsulates non-deterministic data needed for consensus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NonDeterministicData {
    version: u16,
    bitcoin_block_hash: bitcoin::hash_types::BlockHash,
    aggregated_public_key: secp256k1::PublicKey,
    block_fee_recipient_address: Option<Address>,
    runtime_version: RuntimeVersion,
    network_upgrade_payload: Option<NetworkUpgradePayload>,
    multisig_message: Option<multisig_manager::Message>,
}

impl NonDeterministicData {
    /// Returns the version based on whether a fee recipient address is present.
    pub(crate) fn version(&self) -> u16 {
        self.version
    }

    /// Returns the version of the Botanix runtime logic.
    pub(crate) fn runtime_version(&self) -> RuntimeVersion {
        self.runtime_version
    }

    /// The Bitcoin block hash.
    pub(crate) fn bitcoin_block_hash(&self) -> bitcoin::hash_types::BlockHash {
        self.bitcoin_block_hash
    }

    /// The aggregate public key.
    pub(crate) fn aggregated_public_key(&self) -> secp256k1::PublicKey {
        self.aggregated_public_key
    }

    /// The block fee recipient, only available in version 1.
    pub(crate) fn block_fee_recipient_address(&self) -> Option<Address> {
        self.block_fee_recipient_address
    }

    /// The optional multisig message.
    pub(crate) fn multisig_message(
        &self,
    ) -> &Option<multisig_manager::Message> {
        &self.multisig_message
    }

    /// The optional network upgrade payload.
    pub(crate) fn network_upgrade_payload(
        &self,
    ) -> Option<NetworkUpgradePayload> {
        self.network_upgrade_payload
    }

    /// Constructor for version 0 (without a fee recipient).
    #[allow(dead_code)]
    pub(crate) fn new_v0(
        bitcoin_block_hash: bitcoin::hash_types::BlockHash,
        aggregated_public_key: secp256k1::PublicKey,
    ) -> Self {
        Self {
            version: VERSION_0,
            bitcoin_block_hash,
            aggregated_public_key,
            block_fee_recipient_address: None,
            runtime_version: RUNTIME_VERSION_GENESIS,
            network_upgrade_payload: None,
            multisig_message: None,
        }
    }

    /// Constructor for version 1 with a fee recipient.
    #[allow(dead_code)]
    pub(crate) fn new_v1(
        bitcoin_block_hash: bitcoin::hash_types::BlockHash,
        aggregated_public_key: secp256k1::PublicKey,
        block_fee_recipient_address: Address,
    ) -> Self {
        Self {
            version: VERSION_1,
            bitcoin_block_hash,
            aggregated_public_key,
            block_fee_recipient_address: Some(block_fee_recipient_address),
            runtime_version: RUNTIME_VERSION_GENESIS,
            network_upgrade_payload: None,
            multisig_message: None,
        }
    }

    /// Constructor for version 2 with a fee recipient, active runtime version
    /// and an optional network upgrade payload.
    pub(crate) fn new_v2(
        bitcoin_block_hash: bitcoin::hash_types::BlockHash,
        aggregated_public_key: secp256k1::PublicKey,
        block_fee_recipient_address: Address,
        runtime_version: RuntimeVersion,
        network_upgrade_payload: Option<NetworkUpgradePayload>,
    ) -> Self {
        Self {
            version: VERSION_2,
            bitcoin_block_hash,
            aggregated_public_key,
            block_fee_recipient_address: Some(block_fee_recipient_address),
            runtime_version,
            network_upgrade_payload,
            multisig_message: None,
        }
    }

    pub(crate) fn new_v3(
        bitcoin_block_hash: bitcoin::hash_types::BlockHash,
        aggregated_public_key: secp256k1::PublicKey,
        block_fee_recipient_address: Address,
        runtime_version: RuntimeVersion,
        network_upgrade_payload: Option<NetworkUpgradePayload>,
        multisig_message: Option<multisig_manager::Message>,
    ) -> Self {
        Self {
            version: VERSION_3,
            bitcoin_block_hash,
            aggregated_public_key,
            block_fee_recipient_address: Some(block_fee_recipient_address),
            runtime_version,
            network_upgrade_payload,
            multisig_message,
        }
    }

    /// Serializes the non-deterministic data.
    pub(crate) fn serialize(&self) -> Result<Vec<u8>, io::Error> {
        let mut writer = Vec::new();
        self.bitcoin_block_hash.consensus_encode(&mut writer)?;
        self.aggregated_public_key.serialize().consensus_encode(&mut writer)?;
        self.version().consensus_encode(&mut writer)?;

        // Version 1 has a block fee recipient address.
        match self.version() {
            VERSION_0 => {
                // Nothing left to do...
            }
            VERSION_1 => {
                // Encode fee recipient address.
                let address = self.block_fee_recipient_address.expect(
                    "fee recipient address must be set for NDD version 1",
                );

                writer.write_all(address.as_slice())?;
            }
            VERSION_2 => {
                // Encode fee recipient address.
                let address = self.block_fee_recipient_address.expect(
                    "fee recipient address must be set for NDD version 2",
                );

                writer.write_all(address.as_slice())?;

                // Serialize runtime version.
                let RuntimeVersion(MajorVersion(major), MinorVersion(minor)) =
                    self.runtime_version;
                (major, minor).consensus_encode(&mut writer)?;

                // Serialize network upgrade payload, if available.
                let Some(upgrade) = self.network_upgrade_payload.as_ref()
                else {
                    0u8.consensus_encode(&mut writer)?; // ~= None
                    return Ok(writer);
                };

                1u8.consensus_encode(&mut writer)?; // ~= Some(_)

                // Serialize upgrade version
                let RuntimeVersion(MajorVersion(major), MinorVersion(minor)) =
                    upgrade.version;
                (major, minor).consensus_encode(&mut writer)?;

                // Serialize upgrade vote
                match upgrade.vote {
                    Vote::Abstain => 0u8.consensus_encode(&mut writer)?,
                    Vote::Nay => 1u8.consensus_encode(&mut writer)?,
                    Vote::Aye => 2u8.consensus_encode(&mut writer)?,
                };

                upgrade.is_compliant.consensus_encode(&mut writer)?;
            }
            VERSION_3 => {
                // Encode fee recipient address.
                let address = self.block_fee_recipient_address.expect(
                    "fee recipient address must be set for NDD version 2",
                );

                writer.write_all(address.as_slice())?;

                // Serialize runtime version.
                let RuntimeVersion(MajorVersion(major), MinorVersion(minor)) =
                    self.runtime_version;
                (major, minor).consensus_encode(&mut writer)?;

                // Serialize network upgrade payload, if available.
                if let Some(upgrade) = self.network_upgrade_payload.as_ref() {
                    1u8.consensus_encode(&mut writer)?; // ~= Some(_)

                    // Serialize upgrade version
                    let RuntimeVersion(
                        MajorVersion(major),
                        MinorVersion(minor),
                    ) = upgrade.version;
                    (major, minor).consensus_encode(&mut writer)?;

                    // Serialize upgrade vote
                    match upgrade.vote {
                        Vote::Abstain => 0u8.consensus_encode(&mut writer)?,
                        Vote::Nay => 1u8.consensus_encode(&mut writer)?,
                        Vote::Aye => 2u8.consensus_encode(&mut writer)?,
                    };

                    upgrade.is_compliant.consensus_encode(&mut writer)?;
                } else {
                    0u8.consensus_encode(&mut writer)?; // ~= None
                }

                // Serialize multisig manager message, if available.
                if let Some(multisig_message) = self.multisig_message.as_ref() {
                    1u8.consensus_encode(&mut writer)?; // ~= Some(_)
                    multisig_message
                        .serialize(&mut writer)
                        .expect("writer must not fail");
                } else {
                    0u8.consensus_encode(&mut writer)?; // ~= None
                }
            }
            _ => unreachable!("invalid NDD version: {}", self.version),
        }

        Ok(writer)
    }

    /// Deserializes the non-deterministic data.
    pub(crate) fn deserialize(
        reader: &mut impl bitcoin::io::Read,
    ) -> Result<Self, NonDeterministicDataDeserializeError> {
        let bitcoin_block_hash = Decodable::consensus_decode(reader)?;

        let pk_bytes = <[u8; 33]>::consensus_decode(reader)?;
        let aggregated_public_key = secp256k1::PublicKey::from_slice(&pk_bytes)
            .map_err(|_e| {
                encode::Error::ParseFailed("malformed aggregate public key")
            })?;

        // Read the version and conditionally read the address.
        let version = u16::consensus_decode(reader)?;
        match version {
            VERSION_0 => {
                // No block fee recipient expected.
                Ok(Self {
                    version,
                    bitcoin_block_hash,
                    aggregated_public_key,
                    block_fee_recipient_address: None,
                    runtime_version: RUNTIME_VERSION_GENESIS,
                    network_upgrade_payload: None,
                    multisig_message: None,
                })
            }
            VERSION_1 => {
                let mut address_bytes = [0u8; 20];
                reader.read_exact(&mut address_bytes).map_err(|_e| {
                    encode::Error::ParseFailed(
                        "malformed block fee recipient address",
                    )
                })?;
                let block_fee_recipient_address = Address::from(address_bytes);

                let mut this = Self {
                    version,
                    bitcoin_block_hash,
                    aggregated_public_key,
                    block_fee_recipient_address: Some(
                        block_fee_recipient_address,
                    ),
                    runtime_version: RUNTIME_VERSION_GENESIS,
                    network_upgrade_payload: None,
                    multisig_message: None,
                };

                // For version 1 this is optional.
                if let Ok((runtime_version, network_upgrade_payload)) =
                    Self::_deserialize_version_2(reader)
                {
                    this.runtime_version = runtime_version;
                    this.network_upgrade_payload = network_upgrade_payload;
                }

                Ok(this)
            }
            VERSION_2 => {
                let mut address_bytes = [0u8; 20];
                reader.read_exact(&mut address_bytes).map_err(|_e| {
                    encode::Error::ParseFailed(
                        "malformed block fee recipient address",
                    )
                })?;
                let block_fee_recipient_address = Address::from(address_bytes);

                // For version 2 this MUST pass.
                let (runtime_version, network_upgrade_payload) =
                    Self::_deserialize_version_2(reader)?;

                Ok(Self {
                    version,
                    bitcoin_block_hash,
                    aggregated_public_key,
                    block_fee_recipient_address: Some(
                        block_fee_recipient_address,
                    ),
                    runtime_version,
                    network_upgrade_payload,
                    multisig_message: None,
                })
            }
            VERSION_3 => {
                let mut address_bytes = [0u8; 20];
                reader.read_exact(&mut address_bytes).map_err(|_e| {
                    encode::Error::ParseFailed(
                        "malformed block fee recipient address",
                    )
                })?;
                let block_fee_recipient_address = Address::from(address_bytes);

                let (runtime_version, network_upgrade_payload) =
                    Self::_deserialize_version_2(reader)?;

                let multisig_message = Self::_deserialize_version_3(reader)?;

                Ok(Self {
                    version,
                    bitcoin_block_hash,
                    aggregated_public_key,
                    block_fee_recipient_address: Some(
                        block_fee_recipient_address,
                    ),
                    runtime_version,
                    network_upgrade_payload,
                    multisig_message,
                })
            }
            _ => Err(NonDeterministicDataDeserializeError::InvalidVersion),
        }
    }
    fn _deserialize_version_2(
        reader: &mut impl bitcoin::io::Read,
    ) -> Result<
        (RuntimeVersion, Option<NetworkUpgradePayload>),
        NonDeterministicDataDeserializeError,
    > {
        // Decode runtime version.
        let runtime_version = <(u16, u16)>::consensus_decode(reader)?.into();

        // Check whether the network upgrade payload is included.
        match u8::consensus_decode(reader)? {
            0 => {
                // Is NOT included, return.
                return Ok((runtime_version, None));
            }
            1 => {
                // Is included, proceed...
            }
            _ => {
                return Err(
                    NonDeterministicDataDeserializeError::InclusionIndicator,
                )
            }
        }

        // Decode payload information
        let upgrade_version = <(u16, u16)>::consensus_decode(reader)?.into();
        let vote = match u8::consensus_decode(reader)? {
            0 => Vote::Abstain,
            1 => Vote::Nay,
            2 => Vote::Aye,
            _ => return Err(
                NonDeterministicDataDeserializeError::NetworkUpgradePayloadVote,
            ),
        };
        let is_compliant = bool::consensus_decode(reader)?;

        let payload = Some(NetworkUpgradePayload {
            version: upgrade_version,
            vote,
            is_compliant,
        });

        Ok((runtime_version, payload))
    }
    fn _deserialize_version_3(
        reader: &mut impl bitcoin::io::Read,
    ) -> Result<
        Option<multisig_manager::Message>,
        NonDeterministicDataDeserializeError,
    > {
        // TODO: This should just use ciborium's Option<_> handling...
        // Check whether the multisig manager message included.
        match u8::consensus_decode(reader)? {
            0 => {
                // Is NOT included, return.
                return Ok(None);
            }
            1 => {
                // Is included, proceed...
            }
            _ => {
                return Err(
                    NonDeterministicDataDeserializeError::InclusionIndicator,
                )
            }
        }

        let msg =
            multisig_manager::Message::deserialize(BitcoinReadAdapter(reader))
                .map_err(|_| {
                    NonDeterministicDataDeserializeError::MultisigManagerMessage
                })?;

        Ok(Some(msg))
    }
}

/// Adapts `bitcoin::io::Read` to `std::io::Read`.
struct BitcoinReadAdapter<'a, R: bitcoin::io::Read>(&'a mut R);

impl<R: bitcoin::io::Read> std::io::Read for BitcoinReadAdapter<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0
            .read(buf)
            .map_err(|_| io::Error::other("bitcoin::io::Read failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use bitcoin::{hashes::Hash, BlockHash};

    #[test]
    fn test_non_deterministic_data_new() {
        let bitcoin_block_hash = BlockHash::all_zeros();
        let pk = secp256k1::PublicKey::from_slice(
            hex::decode("039bef292b80427d355cecb89eda8a50a7d2196a93d73dade5a0c4a07cd334815d")
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        let ndd = NonDeterministicData::new_v0(bitcoin_block_hash, pk);
        assert_eq!(ndd.version, VERSION_0);
        assert_eq!(ndd.bitcoin_block_hash, bitcoin_block_hash);
        assert_eq!(ndd.aggregated_public_key, pk);
        assert_eq!(ndd.block_fee_recipient_address, None);
        assert_eq!(ndd.runtime_version, RUNTIME_VERSION_GENESIS);
        assert_eq!(ndd.network_upgrade_payload, None);
    }

    #[test]
    fn test_non_deterministic_data_new_v1() {
        let bitcoin_block_hash = BlockHash::all_zeros();
        let pk = secp256k1::PublicKey::from_slice(
            hex::decode("039bef292b80427d355cecb89eda8a50a7d2196a93d73dade5a0c4a07cd334815d")
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        let block_fee_recipient_address = Address::parse_checksummed(
            "0x43C8bDCb9AFeBB1D834A7de18CC214a6FD1632d9",
            None,
        )
        .expect("valid address");

        let ndd = NonDeterministicData::new_v1(
            bitcoin_block_hash,
            pk,
            block_fee_recipient_address,
        );
        assert_eq!(ndd.version, VERSION_1);
        assert_eq!(ndd.bitcoin_block_hash, bitcoin_block_hash);
        assert_eq!(ndd.aggregated_public_key, pk);
        assert_eq!(
            ndd.block_fee_recipient_address,
            Some(block_fee_recipient_address)
        );
        assert_eq!(ndd.runtime_version, RUNTIME_VERSION_GENESIS);
        assert_eq!(ndd.network_upgrade_payload, None);
    }

    #[test]
    fn test_non_deterministic_data_new_v2() {
        let bitcoin_block_hash = BlockHash::all_zeros();
        let pk = secp256k1::PublicKey::from_slice(
            hex::decode("039bef292b80427d355cecb89eda8a50a7d2196a93d73dade5a0c4a07cd334815d")
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        let block_fee_recipient_address = Address::parse_checksummed(
            "0x43C8bDCb9AFeBB1D834A7de18CC214a6FD1632d9",
            None,
        )
        .expect("valid address");

        // Without network upgrade payload.
        let runtime_version = RuntimeVersion::new(1, 5);
        let payload = None;

        let ndd = NonDeterministicData::new_v2(
            bitcoin_block_hash,
            pk,
            block_fee_recipient_address,
            runtime_version,
            payload,
        );
        assert_eq!(ndd.version, VERSION_2);
        assert_eq!(ndd.bitcoin_block_hash, bitcoin_block_hash);
        assert_eq!(ndd.aggregated_public_key, pk);
        assert_eq!(
            ndd.block_fee_recipient_address,
            Some(block_fee_recipient_address)
        );
        assert_eq!(ndd.runtime_version, runtime_version);
        assert_eq!(ndd.network_upgrade_payload, None);

        // With network upgrade payload.
        let payload = Some(NetworkUpgradePayload {
            version: RuntimeVersion::new(2, 5),
            vote: Vote::Aye,
            is_compliant: true,
        });

        let ndd = NonDeterministicData::new_v2(
            bitcoin_block_hash,
            pk,
            block_fee_recipient_address,
            runtime_version,
            payload,
        );
        assert_eq!(ndd.version, VERSION_2);
        assert_eq!(ndd.bitcoin_block_hash, bitcoin_block_hash);
        assert_eq!(ndd.aggregated_public_key, pk);
        assert_eq!(
            ndd.block_fee_recipient_address,
            Some(block_fee_recipient_address)
        );
        assert_eq!(ndd.runtime_version, runtime_version);
        assert_eq!(ndd.network_upgrade_payload, payload); // IS SOME
    }

    #[test]
    fn test_non_deterministic_data_serde_v0() {
        let bitcoin_block_hash = BlockHash::all_zeros();
        let pk: secp256k1::PublicKey = secp256k1::PublicKey::from_slice(
            hex::decode("039bef292b80427d355cecb89eda8a50a7d2196a93d73dade5a0c4a07cd334815d")
                .unwrap()
                .as_slice(),
        )
        .unwrap();

        let ndd = NonDeterministicData::new_v0(bitcoin_block_hash, pk);
        let res = ndd.serialize().unwrap();
        let mut reader = io::Cursor::new(res);
        let deserialized =
            NonDeterministicData::deserialize(&mut reader).unwrap();

        assert_eq!(deserialized.version, ndd.version);
        assert_eq!(deserialized.bitcoin_block_hash, ndd.bitcoin_block_hash);
        assert_eq!(
            deserialized.aggregated_public_key,
            ndd.aggregated_public_key
        );
        assert_eq!(
            deserialized.block_fee_recipient_address,
            ndd.block_fee_recipient_address
        );
        assert_eq!(deserialized.runtime_version, ndd.runtime_version);
        assert_eq!(
            deserialized.network_upgrade_payload,
            ndd.network_upgrade_payload
        );
    }

    #[test]
    fn test_non_deterministic_data_serde_v1() {
        let bitcoin_block_hash = BlockHash::all_zeros();
        let pk: secp256k1::PublicKey = secp256k1::PublicKey::from_slice(
            hex::decode("039bef292b80427d355cecb89eda8a50a7d2196a93d73dade5a0c4a07cd334815d")
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        let block_fee_recipient_address = Address::parse_checksummed(
            "0x43C8bDCb9AFeBB1D834A7de18CC214a6FD1632d9",
            None,
        )
        .expect("valid address");

        let ndd = NonDeterministicData::new_v1(
            bitcoin_block_hash,
            pk,
            block_fee_recipient_address,
        );
        let res = ndd.serialize().unwrap();
        let mut reader = io::Cursor::new(res);
        let deserialized =
            NonDeterministicData::deserialize(&mut reader).unwrap();

        assert_eq!(deserialized.version, ndd.version);
        assert_eq!(deserialized.bitcoin_block_hash, ndd.bitcoin_block_hash);
        assert_eq!(
            deserialized.aggregated_public_key,
            ndd.aggregated_public_key
        );
        assert_eq!(
            deserialized.block_fee_recipient_address,
            ndd.block_fee_recipient_address
        );
        assert_eq!(deserialized.runtime_version, ndd.runtime_version);
        assert_eq!(
            deserialized.network_upgrade_payload,
            ndd.network_upgrade_payload
        );
    }

    #[test]
    fn test_non_deterministic_data_serde_v2() {
        let bitcoin_block_hash = BlockHash::all_zeros();
        let pk: secp256k1::PublicKey = secp256k1::PublicKey::from_slice(
            hex::decode("039bef292b80427d355cecb89eda8a50a7d2196a93d73dade5a0c4a07cd334815d")
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        let block_fee_recipient_address = Address::parse_checksummed(
            "0x43C8bDCb9AFeBB1D834A7de18CC214a6FD1632d9",
            None,
        )
        .expect("valid address");

        let runtime_version = RuntimeVersion::new(1, 0);

        let assert_ndd = |network_upgrade_payload: Option<
            NetworkUpgradePayload,
        >| {
            let ndd = NonDeterministicData::new_v2(
                bitcoin_block_hash,
                pk,
                block_fee_recipient_address,
                runtime_version,
                network_upgrade_payload,
            );
            let res = ndd.serialize().unwrap();
            let mut reader = io::Cursor::new(res);
            let deserialized =
                NonDeterministicData::deserialize(&mut reader).unwrap();

            assert_eq!(deserialized.version, VERSION_2);
            assert_eq!(deserialized.bitcoin_block_hash, ndd.bitcoin_block_hash);
            assert_eq!(
                deserialized.aggregated_public_key,
                ndd.aggregated_public_key
            );
            assert_eq!(
                deserialized.block_fee_recipient_address,
                ndd.block_fee_recipient_address
            );
            assert_eq!(deserialized.runtime_version, ndd.runtime_version);
            // Check network upgrade payload.
            assert_eq!(
                deserialized.network_upgrade_payload,
                ndd.network_upgrade_payload
            );
            assert_eq!(
                deserialized.network_upgrade_payload,
                network_upgrade_payload
            );
        };

        // Without network upgrade payload
        let payload = None;
        assert_ndd(payload);

        // With network upgrade payloads
        let payload = Some(NetworkUpgradePayload {
            version: RuntimeVersion::new(2, 5),
            vote: Vote::Abstain,
            is_compliant: false,
        });

        assert_ndd(payload);

        let payload = Some(NetworkUpgradePayload {
            version: RuntimeVersion::new(2, 5),
            vote: Vote::Nay,
            is_compliant: true,
        });

        assert_ndd(payload);

        let payload = Some(NetworkUpgradePayload {
            version: RuntimeVersion::new(2, 5),
            vote: Vote::Aye,
            is_compliant: true,
        });

        assert_ndd(payload);
    }

    #[test]
    fn test_non_deterministic_data_serde_v3() {
        let bitcoin_block_hash = BlockHash::all_zeros();
        let pk: secp256k1::PublicKey = secp256k1::PublicKey::from_slice(
            hex::decode("039bef292b80427d355cecb89eda8a50a7d2196a93d73dade5a0c4a07cd334815d")
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        let block_fee_recipient_address = Address::parse_checksummed(
            "0x43C8bDCb9AFeBB1D834A7de18CC214a6FD1632d9",
            None,
        )
        .expect("valid address");

        let runtime_version = RuntimeVersion::new(1, 0);

        let assert_ndd = |network_upgrade_payload: Option<
            NetworkUpgradePayload,
        >,
                          multisig_message: Option<
            multisig_manager::Message,
        >| {
            let ndd = NonDeterministicData::new_v3(
                bitcoin_block_hash,
                pk,
                block_fee_recipient_address,
                runtime_version,
                network_upgrade_payload,
                multisig_message,
            );
            let res = ndd.serialize().unwrap();
            let mut reader = io::Cursor::new(res);
            let deserialized =
                NonDeterministicData::deserialize(&mut reader).unwrap();

            assert_eq!(deserialized.version, VERSION_3);
            assert_eq!(deserialized.bitcoin_block_hash, ndd.bitcoin_block_hash);
            assert_eq!(
                deserialized.aggregated_public_key,
                ndd.aggregated_public_key
            );
            assert_eq!(
                deserialized.block_fee_recipient_address,
                ndd.block_fee_recipient_address
            );
            assert_eq!(deserialized.runtime_version, ndd.runtime_version);
            assert_eq!(
                deserialized.network_upgrade_payload,
                ndd.network_upgrade_payload
            );
            assert_eq!(deserialized.multisig_message, ndd.multisig_message);
        };

        // Build a Sunsetting message using the attested_legacy_multisig data.
        let legacy = multisig_manager::attested_legacy_multisig();
        let sunsetting_msg = multisig_manager::Message::Sunsetting {
            multisig_id: legacy.multisig_id,
            coordinator_signature: secp256k1::ecdsa::Signature::from_compact(
                &[1u8; 64],
            )
            .unwrap(),
        };

        // No upgrade payload, no multisig message.
        assert_ndd(None, None);

        // No upgrade payload, with multisig message.
        assert_ndd(None, Some(sunsetting_msg.clone()));

        // With upgrade payload, no multisig message.
        let payload = Some(NetworkUpgradePayload {
            version: RuntimeVersion::new(2, 5),
            vote: Vote::Aye,
            is_compliant: true,
        });
        assert_ndd(payload, None);

        // With upgrade payload and multisig message.
        let payload = Some(NetworkUpgradePayload {
            version: RuntimeVersion::new(2, 5),
            vote: Vote::Nay,
            is_compliant: false,
        });
        assert_ndd(payload, Some(sunsetting_msg));
    }
}
