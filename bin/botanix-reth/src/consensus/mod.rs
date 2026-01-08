//! A [Consensus] implementation of Clique Proof of Authority (POA)
//! that authoritymatically seals blocks.
use async_trait as _;
use botanix_btc_wallet::fallback::FallbackBitcoindClient;
use botanix_chainspec::BotanixChainSpec;

use btcserverlib::database::{MultisigId, LEGACY_MULTISIG_ID};
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
pub struct Storage<RDB, BDB> {
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
        let aggregate_public_key = if let Some(aggregate_public_key) =
            aggregate_public_key
        {
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
    pub(crate) aggregate_public_key:
        Option<BTreeMap<MultisigId, secp256k1::PublicKey>>,
    /// Suggests if we are currently syncing blocks
    pub(crate) is_block_syncing: bool,
}
