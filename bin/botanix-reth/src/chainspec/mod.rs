//! Chain specification for Botanix
use crate::hardforks::{botanix::BotanixHardfork, BotanixHardforks};
use alloy_consensus::Header;
use alloy_eips::eip7840::BlobParams;
use alloy_genesis::Genesis;
use alloy_primitives::{Address, B256, U256};
use botanix_chainspec::constants::BOTANIX_MAINNET_CHAIN_ID;
use reth_chainspec::{
    BaseFeeParams, ChainSpec, DepositContract, EthChainSpec, EthereumHardfork, EthereumHardforks,
    ForkCondition, ForkFilter, ForkId, Hardforks, Head,
};
use reth_discv4::NodeRecord;
use reth_evm::eth::spec::EthExecutorSpec;
use std::{fmt::Display, sync::Arc};

pub mod botanix;
pub mod botanix_testnet;
pub mod parser;

pub use botanix_testnet::botanix_testnet;

/// Botanix chain spec type.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BotanixChainSpec {
    /// [`ChainSpec`].
    pub inner: ChainSpec,
}

impl EthChainSpec for BotanixChainSpec {
    type Header = Header;

    fn blob_params_at_timestamp(&self, timestamp: u64) -> Option<BlobParams> {
        // Botanix doesn't modify blob params in Prague, while ETH does.
        // This is a key difference between Botanix and ETH chain specifications.
        if self.inner.is_cancun_active_at_timestamp(timestamp) {
            Some(self.inner.blob_params.cancun)
        } else {
            None
        }
    }

    fn final_paris_total_difficulty(&self) -> Option<U256> {
        self.inner.final_paris_total_difficulty()
    }

    fn chain(&self) -> alloy_chains::Chain {
        self.inner.chain()
    }

    fn base_fee_params_at_block(&self, block_number: u64) -> BaseFeeParams {
        self.inner.base_fee_params_at_block(block_number)
    }

    fn base_fee_params_at_timestamp(&self, timestamp: u64) -> BaseFeeParams {
        self.inner.base_fee_params_at_timestamp(timestamp)
    }

    fn deposit_contract(&self) -> Option<&DepositContract> {
        None
    }

    fn genesis_hash(&self) -> B256 {
        self.inner.genesis_hash()
    }

    fn prune_delete_limit(&self) -> usize {
        self.inner.prune_delete_limit()
    }

    fn display_hardforks(&self) -> Box<dyn Display> {
        Box::new(self.inner.display_hardforks())
    }

    fn genesis_header(&self) -> &Header {
        self.inner.genesis_header()
    }

    fn genesis(&self) -> &Genesis {
        self.inner.genesis()
    }

    fn bootnodes(&self) -> Option<Vec<NodeRecord>> {
        match self.inner.chain().id() {
            BOTANIX_MAINNET_CHAIN_ID => {
                Some(crate::node::network::bootnodes::botanix_mainnet_nodes())
            }
            BOTANIX_TESTNET_CHAIN_ID => {
                Some(crate::node::network::bootnodes::botanix_testnet_nodes())
            }
            _ => None,
        }
    }

    fn is_optimism(&self) -> bool {
        false
    }
}

impl Hardforks for BotanixChainSpec {
    fn fork<H: reth_chainspec::Hardfork>(&self, fork: H) -> reth_chainspec::ForkCondition {
        self.inner.fork(fork)
    }

    fn forks_iter(
        &self,
    ) -> impl Iterator<Item = (&dyn reth_chainspec::Hardfork, reth_chainspec::ForkCondition)> {
        self.inner.forks_iter()
    }

    fn fork_id(&self, head: &Head) -> ForkId {
        self.inner.fork_id(head)
    }

    fn latest_fork_id(&self) -> ForkId {
        self.inner.latest_fork_id()
    }

    fn fork_filter(&self, head: Head) -> ForkFilter {
        self.inner.fork_filter(head)
    }
}

impl From<ChainSpec> for BotanixChainSpec {
    fn from(value: ChainSpec) -> Self {
        Self { inner: value }
    }
}

impl EthereumHardforks for BotanixChainSpec {
    fn ethereum_fork_activation(&self, fork: EthereumHardfork) -> ForkCondition {
        self.inner.ethereum_fork_activation(fork)
    }
}

impl BotanixHardforks for BotanixChainSpec {
    fn botanix_fork_activation(&self, fork: BotanixHardfork) -> ForkCondition {
        self.fork(fork)
    }
}

impl EthExecutorSpec for BotanixChainSpec {
    fn deposit_contract_address(&self) -> Option<Address> {
        None
    }
}

impl BotanixChainSpec {
    /// Get the head information for this chain spec
    pub fn head(&self) -> Head {
        match self.inner.chain().id() {
            BOTANIX_MAINNET_CHAIN_ID => botanix::head(),
            BOTANIX_TESTNET_CHAIN_ID => botanix_testnet::head(),
            _ => botanix::head(),
        }
    }
}

impl From<BotanixChainSpec> for ChainSpec {
    fn from(value: BotanixChainSpec) -> Self {
        value.inner
    }
}

impl BotanixHardforks for Arc<BotanixChainSpec> {
    fn botanix_fork_activation(&self, fork: BotanixHardfork) -> ForkCondition {
        self.as_ref().botanix_fork_activation(fork)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chainspec::botanix_testnet::botanix_testnet;

    #[test]
    fn test_blob_params_at_timestamp() {
        let chain_spec = BotanixChainSpec::from(botanix_testnet());

        // Test timestamp before Cancun (Cancun activates at 1713330442 on testnet)
        let before_cancun_timestamp = 1713330441;
        let result = chain_spec.blob_params_at_timestamp(before_cancun_timestamp);
        assert!(result.is_none(), "Should return None for timestamp before Cancun");

        // Test timestamp during Cancun (between Cancun and Prague)
        // Prague activates at 1740452880 on testnet
        let during_cancun_timestamp = 1713330442; // Cancun activation time
        let result = chain_spec.blob_params_at_timestamp(during_cancun_timestamp);
        assert!(result.is_some(), "Should return Some for timestamp during Cancun");
        if let Some(blob_params) = result {
            // Check the correct blob param values
            assert_eq!(blob_params.target_blob_count, 3);
            assert_eq!(blob_params.max_blob_count, 6);
        }

        // Test timestamp after Prague activation
        let after_prague_timestamp = 1740452880; // Prague activation time
        let result = chain_spec.blob_params_at_timestamp(after_prague_timestamp);
        // Botanix doesn't modify blob params in Prague, so should still return Cancun params
        assert!(
            result.is_some(),
            "Should return Some for timestamp after Prague (Botanix doesn't modify blob params)"
        );
        if let Some(blob_params) = result {
            // Check the correct blob param values (should be same as Cancun)
            assert_eq!(blob_params.target_blob_count, 3);
            assert_eq!(blob_params.max_blob_count, 6);
        }

        // Test timestamp well after Prague
        let well_after_prague_timestamp = 1740452881;
        let result = chain_spec.blob_params_at_timestamp(well_after_prague_timestamp);
        assert!(result.is_some(), "Should return Some for timestamp well after Prague");
        if let Some(blob_params) = result {
            // Check the correct blob param values (should be same as Cancun)
            assert_eq!(blob_params.target_blob_count, 3);
            assert_eq!(blob_params.max_blob_count, 6);
        }
    }
}
