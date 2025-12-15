//! A [Consensus] implementation of Clique Proof of Authority (POA)
//! that authoritymatically seals blocks.
use async_trait as _;
use botanix_btc_wallet::fallback::FallbackBitcoindClient;
use botanix_chainspec::BotanixChainSpec;

use btcserverlib::database::LEGACY_MULTISIG_ID;
use bytes as _;
use displaydoc as _;
use reth_network_peers as _;
use reth_node_core as _;
use serde_json as _;
use std::{collections::BTreeMap, net::SocketAddr, sync::Arc};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
mod builder;

/// Comet BFT abci and consensus driver
pub mod comet_bft;
mod excecution_utils;
mod frost_task;
mod signing;
pub mod snapshot_manager;
pub mod utils;
pub use builder::AuthorityConsensusBuilder;

use crate::node::evm::config::BotanixEvmConfig;
pub mod test_utils;
pub mod wallet_state_sync;

/// Maximum extra data size in a block which supports Botanix consensus rules.
/// This is larger than the Ethereum default of 32 bytes.
pub const MAXIMUM_EXTRA_DATA_SIZE: usize = 256;

/// Max EDH size; for specific details see [ExtraDataHeader]
pub const MAX_EDH_SIZE: usize = 93;
/// In memory storage
/// All this struct does is provide a rwlock wrapper around the storage inner
#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct Storage<RDB, BDB> {
    /// Reth Database Provider Factory
    pub(crate) reth_database: RDB,
    /// Botanix Database Provider Factory
    pub(crate) botanix_database_factory: BDB,
    /// The authority list in the genesis block
    pub(crate) genesis_authorities: Vec<secp256k1::PublicKey>,
    /// keep track of my place among the signer
    /// This will change as new signers are removed
    pub(crate) signer_index: usize,
    /// Authority Signer public key
    pub(crate) authority: secp256k1::PublicKey,
    /// Bitcoin network
    pub(crate) btc_network: bitcoin::Network,
    /// Authority socket addresses pulled from federation config
    pub(crate) authority_socket_addresses: Vec<SocketAddr>,
    /// Evm config
    pub(crate) evm_config: BotanixEvmConfig,
    /// Bitcoind Factory
    pub(crate) bitcoind_factory: Arc<FallbackBitcoindClient>,
    /// Chain spec
    pub(crate) chain_spec: Arc<BotanixChainSpec>,
    // The inner storage, everything here is rw locked
    pub(crate) inner: Arc<RwLock<StorageInner>>,
}

impl<RDB: Clone, BDB: Clone> Storage<RDB, BDB> {
    /// Create a new instance of the storage
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        genesis_authorities: Vec<secp256k1::PublicKey>,
        signer_index: usize,
        authority: secp256k1::PublicKey,
        btc_network: bitcoin::Network,
        aggregate_public_key: Option<secp256k1::PublicKey>,
        authority_socket_addresses: Vec<SocketAddr>,
        evm_config: BotanixEvmConfig,
        chain_spec: Arc<BotanixChainSpec>,
        bitcoind_factory: Arc<FallbackBitcoindClient>,
        reth_database: RDB,
        botanix_database_factory: BDB,
    ) -> Self {
        // TODO: use the correct multisig_id
        let aggregate_public_key = if let Some(aggregate_public_key) = aggregate_public_key {
            Some(BTreeMap::from([(LEGACY_MULTISIG_ID, aggregate_public_key)]))
        } else {
            None
        };

        let storage_inner =
            StorageInner { aggregate_public_key, is_block_syncing: false };

        Self {
            reth_database,
            botanix_database_factory,
            genesis_authorities,
            signer_index,
            authority,
            btc_network,
            authority_socket_addresses,
            evm_config,
            chain_spec,
            bitcoind_factory,
            inner: Arc::new(RwLock::new(storage_inner)),
        }
    }

    /// Returns the write lock of the storage
    pub(crate) async fn write(&self) -> RwLockWriteGuard<'_, StorageInner> {
        self.inner.write().await
    }

    #[allow(dead_code)]
    /// Returns the read lock of the storage
    pub(crate) async fn read(&self) -> RwLockReadGuard<'_, StorageInner> {
        self.inner.read().await
    }
}

#[derive(Debug)]
/// In-memory storage for the chain the authority seal engine is building.
/// data shared amongst the different tasks should be stored here and protected by a rwlock
pub(crate) struct StorageInner {
    /// The aggregate public key of the FROST threshold signature scheme
    /// Should get populated after DKG
    pub(crate) aggregate_public_key: Option<BTreeMap<u32, secp256k1::PublicKey>>,
    /// Suggests if we are currently syncing blocks
    pub(crate) is_block_syncing: bool,
}

// TODO
// #[cfg(test)]
// mod tests {
//     use botanix_authority_edh::extra_data_header::{ExtraDataHeader, CHAIN_VERSION};
//     use botanix_authority_rsp::{RandomSource, RandomSourceProvider};
//     use botanix_chainspec::constants::BOTANIX_TESTNET;
//     use reth_consensus::InvalidAggregatedPublicKeyError;
//     use botanix_consensus_common::utils::is_inturn;
//     use alloy_primitives::Bytes;
//     use alloy_eips::merge::ALLOWED_FUTURE_BLOCK_TIME_SECONDS;
//     use alloy_consensus::constants::MAXIMUM_EXTRA_DATA_SIZE;
//     use std::str::FromStr;
//     use super::*;

//     #[allow(dead_code)]
//     const EDH_DEFAULT_SIGHASH: &str =
//         "0xaaa3492fe3eec8da1ca35aca5930a44b1a5805e813bdd1773678b5041d905276";

//     #[allow(dead_code)]
//     const SK1: &str = "1aabc5cc52b62b570dc69001f1ab49cd1a7056bf6312fe058f094135f2c9b019";
//     #[allow(dead_code)]
//     const SK2: &str = "1bc1f5cc52b62b570dc69001f1ab49cd1a7056bf6312fe058f094135f2c9b019";

//     // Tests for validating poa extra data header
//     #[test]
//     fn should_skip_over_genesis() {
//         let consensus = AuthorityConsensus::new(Arc::new(BOTANIX_TESTNET.as_ref().to_owned()));
//         let header = Header { number: 0, ..Default::default() };
//         let authority_signers = vec![];
//         // Just use the first key as the dummy agg key
//         let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
//         let dummy_agg_key = sk1.public_key(secp256k1::SECP256K1);

//         let result =
//             consensus.validate_extra_data_header(&header, &authority_signers, Some(&dummy_agg_key));

//         assert!(result.is_ok());
//     }

//     #[test]
//     fn fails_when_edh_exceeds_max_size() {
//         let consensus = AuthorityConsensus::new(Arc::new(BOTANIX_TESTNET.as_ref().to_owned()));
//         // In this case we are signing with a non federation different key
//         let mut edh = ExtraDataHeader::default();
//         let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();

//         // Just use the first key as the dummy agg key
//         let dummy_agg_key = sk1.public_key(secp256k1::SECP256K1);
//         edh.aggregated_public_key = dummy_agg_key;

//         let authority_signers = vec![sk1.public_key(secp256k1::SECP256K1)];
//         let header = Header {
//             number: 1,
//             extra_data: Bytes::from([1; MAXIMUM_EXTRA_DATA_SIZE + 1]),
//             ..Default::default()
//         };

//         let result =
//             consensus.validate_extra_data_header(&header, &authority_signers, Some(&dummy_agg_key));
//         assert!(result.is_err());
//         assert_eq!(
//             result.err().unwrap(),
//             ConsensusError::ExtraDataExceedsMax { len: MAXIMUM_EXTRA_DATA_SIZE + 1 }
//         );
//     }

//     #[test]
//     fn fails_when_edh_has_no_agg_pk() {
//         let consensus = AuthorityConsensus::new(Arc::new(BOTANIX_TESTNET.as_ref().to_owned()));
//         let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
//         let authority_signers = vec![sk1.public_key(secp256k1::SECP256K1)];
//         let header = Header { number: 1, ..Default::default() };

//         let result = consensus.validate_extra_data_header(&header, &authority_signers, None);
//         assert!(result.is_err());
//         assert_eq!(
//             result.err().unwrap(),
//             ConsensusError::InvalidAggregatedPublicKey(
//                 InvalidAggregatedPublicKeyError::MissingAggregatedPublicKey
//             )
//         );
//     }

//     #[test]
//     fn fails_with_invalid_edh() {
//         let consensus = AuthorityConsensus::new(Arc::new(BOTANIX_TESTNET.as_ref().to_owned()));
//         // Just use the first key as the dummy agg key
//         let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
//         let dummy_agg_key = sk1.public_key(secp256k1::SECP256K1);

//         let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
//         let authority_signers = vec![sk1.public_key(secp256k1::SECP256K1)];
//         let header = Header { number: 1, extra_data: Bytes::from([0; 64]), ..Default::default() };

//         let result =
//             consensus.validate_extra_data_header(&header, &authority_signers, Some(&dummy_agg_key));
//         assert!(result.is_err());
//         assert_eq!(result.err().unwrap(), ConsensusError::ExtraDataInvalid,);
//     }

//     #[test]
//     fn should_not_accept_edh_with_nums_point_past_genesis() {
//         let consensus = AuthorityConsensus::new(Arc::new(BOTANIX_TESTNET.as_ref().to_owned()));
//         // By default edh will use the nums point
//         let edh = ExtraDataHeader::default();

//         // Just use the first key as the dummy agg key
//         let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
//         let dummy_agg_key = sk1.public_key(secp256k1::SECP256K1);

//         let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
//         let authority_signers = vec![sk1.public_key(secp256k1::SECP256K1)];
//         let header =
//             Header { number: 1, extra_data: Bytes::from(edh.serialize()), ..Default::default() };

//         let result =
//             consensus.validate_extra_data_header(&header, &authority_signers, Some(&dummy_agg_key));
//         assert_eq!(
//             result.err().unwrap(),
//             ConsensusError::InvalidAggregatedPublicKey(
//                 InvalidAggregatedPublicKeyError::NumsAggregatePublicKeyPastGenesis
//             )
//         );
//     }

//     #[test]
//     fn should_not_accept_edh_with_exact_nums_point() {
//         let consensus = AuthorityConsensus::new(Arc::new(BOTANIX_TESTNET.as_ref().to_owned()));
//         // By default edh will use the nums point
//         let edh =
//             ExtraDataHeader { aggregated_public_key: nums_secp256k1_pk(), ..Default::default() };
//         let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
//         let authority_signers = vec![sk1.public_key(secp256k1::SECP256K1)];
//         let header =
//             Header { number: 1, extra_data: Bytes::from(edh.serialize()), ..Default::default() };

//         let result = consensus.validate_extra_data_header(
//             &header,
//             &authority_signers,
//             Some(&nums_secp256k1_pk()),
//         );
//         assert_eq!(
//             result.err().unwrap(),
//             ConsensusError::InvalidAggregatedPublicKey(
//                 InvalidAggregatedPublicKeyError::NumsAggregatePublicKeyPastGenesis
//             )
//         );
//     }

//     #[test]
//     fn should_not_accept_edh_with_invalid_agg_pk() {
//         let consensus = AuthorityConsensus::new(Arc::new(BOTANIX_TESTNET.as_ref().to_owned()));
//         // By default edh will use the nums point
//         let mut edh = ExtraDataHeader::default();

//         // Just use the first key as the dummy agg key
//         let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
//         let dummy_agg_key = sk1.public_key(secp256k1::SECP256K1);

//         edh.aggregated_public_key = dummy_agg_key;

//         let different_key = secp256k1::SecretKey::from_str(SK2).unwrap();
//         let different_pk = different_key.public_key(secp256k1::SECP256K1);

//         let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
//         let authority_signers = vec![sk1.public_key(secp256k1::SECP256K1)];
//         let header =
//             Header { number: 1, extra_data: Bytes::from(edh.serialize()), ..Default::default() };

//         let result =
//             consensus.validate_extra_data_header(&header, &authority_signers, Some(&different_pk));
//         assert_eq!(
//             result.err().unwrap(),
//             ConsensusError::InvalidAggregatedPublicKey(
//                 InvalidAggregatedPublicKeyError::InvalidAggregatedPublicKey
//             )
//         );
//     }

//     #[test]
//     fn unix_timestamp() {
//         let timestamp = botanix_consensus_common::utils::unix_timestamp();
//         assert!(timestamp > 0);
//     }

//     #[test]
//     fn should_validate_poa_block_beneficiary() {
//         // default beneficiary is the burn address
//         let consensus = AuthorityConsensus::new(Arc::new(BOTANIX_TESTNET.as_ref().to_owned()));
//         let header = Header::default();
//         let result = consensus.validate_block_beneficiary(&header);
//         assert!(result.is_ok());
//     }

//     #[test]
//     fn should_fail_validate_poa_block_beneficiary() {
//         let consensus = AuthorityConsensus::new(Arc::new(BOTANIX_TESTNET.as_ref().to_owned()));
//         let header = Header {
//             beneficiary: Address::from_str("0x4e0f6e05C8ca4b3dc2B7b7Ad6249B149b1980394").unwrap(),
//             ..Default::default()
//         };
//         let result = consensus.validate_block_beneficiary(&header);
//         assert!(result.is_err());
//     }

//     #[test]
//     fn is_inturn_true() {
//         let authorities_len = 1;
//         let signer_index = 0;
//         let random_source = RandomSourceProvider::new().random_source();
//         assert!(is_inturn(
//             authorities_len,
//             signer_index,
//             ALLOWED_FUTURE_BLOCK_TIME_SECONDS,
//             random_source
//         ));
//     }

//     #[test]
//     fn is_inturn_false() {
//         let authorities_len = 1;
//         let signer_index = 1;
//         let random_source = RandomSourceProvider::new().random_source();

//         assert!(!is_inturn(
//             authorities_len,
//             signer_index,
//             ALLOWED_FUTURE_BLOCK_TIME_SECONDS,
//             random_source
//         ));
//     }

//     #[test]
//     fn should_get_block_fee_recipient_address_from_header() {
//         let mut header = Header::default();
//         let edh = ExtraDataHeader::default();
//         header.add_extra_data_header(&edh);
//         let block_fee_recipient_address = header.block_fee_recipient_address().unwrap();
//         assert_eq!(block_fee_recipient_address, Address::ZERO);

//         let mut header2 = Header::default();
//         let edh2 = ExtraDataHeader {
//             block_fee_recipient_address: Address::from_str(
//                 "0x4e0f6e05C8ca4b3dc2B7b7Ad6249B149b1980394",
//             )
//             .unwrap(),
//             ..Default::default()
//         };
//         header2.add_extra_data_header(&edh2);
//         let block_producer_address2 = header2.block_fee_recipient_address().unwrap();
//         assert_eq!(block_producer_address2, edh2.block_fee_recipient_address);
//     }

//     #[test]
//     fn should_validate_chain_version() {
//         let edh_chain_version = CHAIN_VERSION;
//         let result = validate_chain_version(edh_chain_version);
//         assert!(result.is_ok());

//         let edh_chain_version = CHAIN_VERSION + 1;
//         let result = validate_chain_version(edh_chain_version);
//         assert!(result.is_err());
//     }
// }
