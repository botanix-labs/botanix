#![allow(unused)]
use alloy_chains::Chain;
use core::any::Any;
use reth_chainspec::ForkCondition;
use reth_ethereum_forks::{
    hardfork, ChainHardforks, EthereumHardfork, Hardfork,
};
use revm::primitives::hardfork::SpecId;

hardfork!(
    /// The name of a Botanix hardfork.
    ///
    /// When building a list of hardforks for a chain, it's still expected to mix with [`EthereumHardfork`].
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Default)]
    BotanixHardfork {
        /// Botanix `Prague` hardfork
        #[default]
        Prague,
    }
);

impl BotanixHardfork {
    /// Botanix mainnet list of hardforks.
    pub fn botanix_mainnet() -> ChainHardforks {
        ChainHardforks::new(vec![
            (EthereumHardfork::Frontier.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Homestead.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Tangerine.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::SpuriousDragon.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Byzantium.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Constantinople.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Petersburg.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Istanbul.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::MuirGlacier.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Berlin.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::London.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Paris.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Shanghai.boxed(), ForkCondition::Timestamp(0)),
            (EthereumHardfork::Cancun.boxed(), ForkCondition::Timestamp(0)),
            (EthereumHardfork::Prague.boxed(), ForkCondition::Timestamp(0)),
        ])
    }

    /// Botanix testnet list of hardforks.
    pub fn botanix_testnet() -> ChainHardforks {
        ChainHardforks::new(vec![
            (EthereumHardfork::Frontier.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Homestead.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Tangerine.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::SpuriousDragon.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Byzantium.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Constantinople.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Petersburg.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Istanbul.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::MuirGlacier.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Berlin.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::London.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Paris.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Shanghai.boxed(), ForkCondition::Timestamp(0)),
            (EthereumHardfork::Cancun.boxed(), ForkCondition::Timestamp(0)),
            (EthereumHardfork::Prague.boxed(), ForkCondition::Timestamp(0)),
        ])
    }
}

impl From<BotanixHardfork> for SpecId {
    fn from(spec: BotanixHardfork) -> Self {
        match spec {
            BotanixHardfork::Prague => SpecId::PRAGUE,
        }
    }
}

impl From<SpecId> for BotanixHardfork {
    fn from(spec: SpecId) -> Self {
        match spec {
            SpecId::PRAGUE => BotanixHardfork::Prague,
            _ => BotanixHardfork::Prague, // Default to Prague for unknown specs
        }
    }
}
