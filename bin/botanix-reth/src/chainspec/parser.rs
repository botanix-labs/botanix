//! Parser utilities for Botanix chain specification.
use super::{botanix::botanix_mainnet, botanix_testnet::botanix_testnet, BotanixChainSpec};
use reth_cli::chainspec::ChainSpecParser;
use std::sync::Arc;

/// Botanix chain specification parser.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct BotanixChainSpecParser;

impl ChainSpecParser for BotanixChainSpecParser {
    type ChainSpec = BotanixChainSpec;

    const SUPPORTED_CHAINS: &'static [&'static str] = &["botanix-mainnet", "botanix-testnet"];

    fn parse(s: &str) -> eyre::Result<Arc<Self::ChainSpec>> {
        chain_value_parser(s)
    }
}

/// Clap value parser for [`BotanixChainSpec`]s.
///
/// The value parser matches either a known chain, the path
/// to a json file, or a json formatted string in-memory. The json needs to be a Genesis struct.
pub fn chain_value_parser(s: &str) -> eyre::Result<Arc<BotanixChainSpec>> {
    match s {
        "botanix-mainnet" => Ok(Arc::new(BotanixChainSpec { inner: botanix_mainnet() })),
        "botanix-testnet" => Ok(Arc::new(BotanixChainSpec { inner: botanix_testnet() })),
        _ => Err(eyre::eyre!("Unsupported chain: {}", s)),
    }
}
