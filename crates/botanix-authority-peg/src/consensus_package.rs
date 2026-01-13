//! Botanix Specific primitives

use std::collections::HashSet;

/// Helper type for the recent header
/// Second field is the height of the header
pub type RecentHeader = (bitcoin::block::Header, u32);

#[derive(Debug, Clone)]
/// Series of botanix specific consensus data
pub struct BotanixConsensusPackage {
    /// Deeply confirmed bitcoin header.
    pub bitcoin_checkpoint: RecentHeader,
    /// Aggregated public keys
    pub aggregated_public_keys: HashSet<secp256k1::PublicKey>,
    /// Bitcoin network
    pub btc_network: bitcoin::Network,
}
