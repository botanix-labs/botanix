//! Hard forks of Botanix protocol.
#![allow(unused)]
pub use botanix::BotanixHardfork;

use reth_chainspec::{EthereumHardforks, ForkCondition};

pub mod botanix;

/// Extends [`EthereumHardforks`] with Botanix helper methods.
pub trait BotanixHardforks: EthereumHardforks {
    /// Retrieves [`ForkCondition`] by an [`BotanixHardfork`]. If `fork` is not
    /// present, returns [`ForkCondition::Never`].
    fn botanix_fork_activation(&self, fork: BotanixHardfork) -> ForkCondition;
}
