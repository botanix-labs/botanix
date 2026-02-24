//! A [Consensus] implementation of Clique Proof of Authority (POA)
//! that authoritymatically seals blocks.
use async_trait as _;
use botanix_btc_wallet::fallback::FallbackBitcoindClient;
use botanix_chainspec::BotanixChainSpec;

use bytes as _;
use displaydoc as _;
use reth_network_peers as _;
use reth_node_core as _;
use serde_json as _;
use std::sync::Arc;
mod builder;
mod operator;

/// Comet BFT abci and consensus driver
pub mod comet_bft;
mod execution_utils;
mod frost_task;
pub mod multisig_manager;
mod signing;
pub mod snapshot_manager;
pub mod utils;
pub use builder::AuthorityConsensusBuilder;
pub use operator::OperatorBuilder;

use crate::node::evm::config::BotanixEvmConfig;
pub mod test_utils;
pub mod wallet_state_sync;

/// Maximum extra data size in a block which supports Botanix consensus rules.
/// This is larger than the Ethereum default of 32 bytes.
pub const MAXIMUM_EXTRA_DATA_SIZE: usize = 256;

/// Max EDH size; for specific details see [ExtraDataHeader]
pub const MAX_EDH_SIZE: usize = 93;

/// In memory storage
/// TODO: Consider deprecating this entirely.
#[derive(Clone)]
#[allow(missing_debug_implementations)]
pub struct Storage<RDB, BDB> {
    /// Reth Database Provider Factory
    pub(crate) reth_database: RDB,
    /// Botanix Database Provider Factory
    pub(crate) botanix_database_factory: BDB,
    /// Bitcoin network
    pub(crate) btc_network: bitcoin::Network,
    /// Evm config
    pub(crate) evm_config: BotanixEvmConfig,
    /// Bitcoind Factory
    pub(crate) bitcoind_factory: Arc<FallbackBitcoindClient>,
    /// Chain spec
    pub(crate) chain_spec: Arc<BotanixChainSpec>,
}

impl<RDB: Clone, BDB: Clone> Storage<RDB, BDB> {
    /// Create a new instance of the storage
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        btc_network: bitcoin::Network,
        evm_config: BotanixEvmConfig,
        chain_spec: Arc<BotanixChainSpec>,
        bitcoind_factory: Arc<FallbackBitcoindClient>,
        reth_database: RDB,
        botanix_database_factory: BDB,
    ) -> Self {
        Self {
            reth_database,
            botanix_database_factory,
            btc_network,
            evm_config,
            chain_spec,
            bitcoind_factory,
        }
    }
}
