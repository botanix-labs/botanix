//! Hard forks of Botanix protocol.
#![allow(unused)]
use botanix::BotanixHardfork;
use reth_chainspec::{EthereumHardforks, ForkCondition};

pub mod botanix;

/// Extends [`EthereumHardforks`] with Botanix helper methods.
pub trait BotanixHardforks: EthereumHardforks {
    /// Retrieves [`ForkCondition`] by an [`BotanixHardfork`]. If `fork` is not present, returns
    /// [`ForkCondition::Never`].
    fn botanix_fork_activation(&self, fork: BotanixHardfork) -> ForkCondition;

    /// Convenience method to check if [`BotanixHardfork::Jalapeno`] is firstly active at a given
    /// block.
    fn is_jalapeno_transition_at_block(&self, block_number: u64) -> bool {
        self.botanix_fork_activation(BotanixHardfork::Jalapeno).transitions_at_block(block_number)
    }

    /// Convenience method to check if [`BotanixHardfork::Jalapeno`] is active at a given block.
    fn is_jalapeno_active_at_block(&self, block_number: u64) -> bool {
        self.botanix_fork_activation(BotanixHardfork::Jalapeno).active_at_block(block_number)
    }

    /// Convenience method to check if [`BotanixHardfork::Pectra`] is firstly active at a given
    /// timestamp and parent timestamp.
    fn is_pectra_transition_at_timestamp(&self, timestamp: u64, parent_timestamp: u64) -> bool {
        self.botanix_fork_activation(BotanixHardfork::Pectra)
            .transitions_at_timestamp(timestamp, parent_timestamp)
    }

    /// Convenience method to check if [`BotanixHardfork::Pectra`] is active at a given timestamp.
    fn is_pectra_active_at_timestamp(&self, timestamp: u64) -> bool {
        self.botanix_fork_activation(BotanixHardfork::Pectra).active_at_timestamp(timestamp)
    }
}
