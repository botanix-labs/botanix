#![allow(unused)]
use alloy_chains::Chain;
use core::any::Any;
use reth_chainspec::ForkCondition;
use reth_ethereum_forks::{hardfork, ChainHardforks, EthereumHardfork, Hardfork};
use revm::primitives::hardfork::SpecId;

hardfork!(
    /// The name of a Botanix hardfork.
    ///
    /// When building a list of hardforks for a chain, it's still expected to mix with [`EthereumHardfork`].
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Default)]
    BotanixHardfork {
        /// Botanix `Jalapeno` hardfork
        Jalapeno,
        /// Botanix `Pectra` hardfork
        #[default]
        Pectra,
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
            (EthereumHardfork::Berlin.boxed(), ForkCondition::Block(31302048)),
            (EthereumHardfork::London.boxed(), ForkCondition::Block(31302048)),
            (EthereumHardfork::Shanghai.boxed(), ForkCondition::Timestamp(1705996800)), /* 2024-01-23 08:00:00 AM UTC */
            (EthereumHardfork::Cancun.boxed(), ForkCondition::Timestamp(1718863500)), /* 2024-06-20 06:05:00 AM UTC */
            (EthereumHardfork::Prague.boxed(), ForkCondition::Timestamp(1742436600)), /* 2025-03-20 02:10:00 AM UTC */
            (Self::Jalapeno.boxed(), ForkCondition::Block(29020050)),
            (Self::Pectra.boxed(), ForkCondition::Timestamp(1792436600)), // in the future (2026-12-20 02:10:00 AM UTC)
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
            (EthereumHardfork::Shanghai.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Cancun.boxed(), ForkCondition::Block(0)),
            (EthereumHardfork::Prague.boxed(), ForkCondition::Block(0)),
            (Self::Jalapeno.boxed(), ForkCondition::Block(29020050)),
            (Self::Pectra.boxed(), ForkCondition::Timestamp(1792436600)),  // in the future (2026-12-20 02:10:00 AM UTC)
        ])
    }
}

/// Match helper method since it's not possible to match on `dyn Hardfork`
fn match_hardfork<H, HF, BHF>(fork: H, hardfork_fn: HF, botanix_hardfork_fn: BHF) -> Option<u64>
where
    H: Hardfork,
    HF: Fn(&EthereumHardfork) -> Option<u64>,
    BHF: Fn(&BotanixHardfork) -> Option<u64>,
{
    let fork: &dyn Any = &fork;
    if let Some(fork) = fork.downcast_ref::<EthereumHardfork>() {
        return hardfork_fn(fork)
    }
    fork.downcast_ref::<BotanixHardfork>().and_then(botanix_hardfork_fn)
}

impl From<BotanixHardfork> for SpecId {
    fn from(spec: BotanixHardfork) -> Self {
        match spec {
            BotanixHardfork::Jalapeno | BotanixHardfork::Pectra => SpecId::PRAGUE,
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::chainspec::{botanix::botanix_mainnet, botanix_testnet::botanix_testnet};

//     #[test]
//     fn test_hardfork_activation_order_differences() {
//         // Test the critical difference between mainnet and testnet activation orders
//         // This demonstrates why the order in revm_spec_by_timestamp_and_block_number matters

//         // Test mainnet chain spec
//         let mainnet_spec = crate::chainspec::BotanixChainSpec::from(botanix_mainnet());

//         // Test blocks around the critical transition points
//         // Block 23846000: Should be Jalapeno (before Jalapeno activation)
//         assert_eq!(
//             crate::node::evm::config::revm_spec_by_timestamp_and_block_number(
//                 mainnet_spec.clone(),
//                 1700000000, // Some timestamp
//                 23846000
//             ),
//             BotanixHardfork::Jalapeno
//         );

//         // Block 23846001: Should be Pectra (Pectra activation block)
//         assert_eq!(
//             crate::node::evm::config::revm_spec_by_timestamp_and_block_number(
//                 mainnet_spec.clone(),
//                 1700000000, // Some timestamp
//                 23846001
//             ),
//             BotanixHardfork::Pectra
//         );
//     }
// }
