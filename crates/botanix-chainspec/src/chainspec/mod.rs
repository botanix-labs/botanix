use alloy_chains::NamedChain;
use alloy_consensus::Header;
use alloy_eips::eip7840::BlobParams;
use alloy_genesis::Genesis;
use alloy_hardforks::Hardfork;
use alloy_primitives::{Address, B256, U256};
use core::fmt::{Debug, Display};
use reth_chainspec::{
    BaseFeeParams, ChainSpec, DepositContract, EthChainSpec, EthereumHardfork,
    EthereumHardforks, ForkFilter, ForkId, Hardforks, Head,
};
use reth_ethereum_forks::ForkCondition;
use reth_evm::eth::spec::EthExecutorSpec;
use reth_network_peers::NodeRecord;
use std::{ops::Deref, sync::Arc};

use crate::{
    constants::{
        botanix_mainnet_head, botanix_testnet_head, BOTANIX_MAINNET_CHAIN_ID,
        BOTANIX_TESTNET_CHAIN_ID,
    },
    BotanixHardfork, BotanixHardforks,
};
pub mod constants;
pub mod parser;

/// Botanix chain spec type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotanixChainSpec {
    /// [`ChainSpec`].
    pub inner: ChainSpec,

    /// The number of confirmations we require for pegins from the mainchain.
    pub bitcoin_checkpoint_confirmation_depth: u32,

    /// How many checkpoints before the strong confirmation depth to keep
    /// (depth < strong) for validation
    pub historical_bitcoin_checkpoints_count: usize,

    /// How many historical checkpoints to keep (depth > strong) for validation
    pub weak_bitcoin_checkpoints_count: usize,

    /// Botanix fee recipient
    pub botanix_fee_recipient: Option<String>,

    /// LST fee receiver
    /// This is the contract address that receives block fees as part of native
    /// staking
    pub lst_fee_receiver: Option<String>,

    /// EIP-225: Clique Proof-of-Authority consensus protocol.
    /// The number of blocks in an epoch for PoA consensus
    pub epoch_length: u64,
}

impl BotanixChainSpec {
    pub fn from_chain_spec(chain_spec: ChainSpec) -> Self {
        Self {
            inner: chain_spec,
            bitcoin_checkpoint_confirmation_depth: Default::default(),
            historical_bitcoin_checkpoints_count: Default::default(),
            weak_bitcoin_checkpoints_count: Default::default(),
            botanix_fee_recipient: None,
            lst_fee_receiver: None,
            epoch_length: Default::default(),
        }
    }
}

impl Default for BotanixChainSpec {
    fn default() -> Self {
        Self {
            inner: ChainSpec::default(),
            botanix_fee_recipient: None,
            lst_fee_receiver: None,
            bitcoin_checkpoint_confirmation_depth: 0,
            weak_bitcoin_checkpoints_count: 0,
            historical_bitcoin_checkpoints_count: 0,
            epoch_length: 0,
        }
    }
}

impl Deref for BotanixChainSpec {
    type Target = ChainSpec;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl BotanixChainSpec {
    pub fn inner(&self) -> &ChainSpec {
        self
    }

    pub fn inner_arc(&self) -> Arc<ChainSpec> {
        self.inner().clone().into()
    }
}

impl EthChainSpec for BotanixChainSpec {
    type Header = Header;

    fn blob_params_at_timestamp(&self, timestamp: u64) -> Option<BlobParams> {
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
        match self.inner.chain().try_into().ok()? {
            NamedChain::Mainnet => {
                Some(vec![]) // TODO: add botanix mainnet nodes
            }
            NamedChain::Sepolia => {
                Some(vec![]) // TODO: add botanix testnet nodes
            }
            _ => None,
        }
    }

    fn is_optimism(&self) -> bool {
        false
    }
}

impl Hardforks for BotanixChainSpec {
    fn fork<H: Hardfork>(&self, fork: H) -> ForkCondition {
        self.inner.fork(fork)
    }

    fn forks_iter(
        &self,
    ) -> impl Iterator<Item = (&dyn Hardfork, ForkCondition)> {
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

impl EthereumHardforks for BotanixChainSpec {
    fn ethereum_fork_activation(
        &self,
        fork: EthereumHardfork,
    ) -> ForkCondition {
        self.inner.ethereum_fork_activation(fork)
    }
}

impl BotanixHardforks for BotanixChainSpec {
    fn botanix_fork_activation(&self, fork: BotanixHardfork) -> ForkCondition {
        self.fork(fork)
    }
}

impl BotanixHardforks for Arc<BotanixChainSpec> {
    fn botanix_fork_activation(&self, fork: BotanixHardfork) -> ForkCondition {
        self.as_ref().botanix_fork_activation(fork)
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
            BOTANIX_MAINNET_CHAIN_ID => botanix_mainnet_head(),
            BOTANIX_TESTNET_CHAIN_ID => botanix_testnet_head(),
            _ => botanix_mainnet_head(),
        }
    }
}

impl From<ChainSpec> for BotanixChainSpec {
    fn from(value: ChainSpec) -> Self {
        Self { inner: value, ..Default::default() }
    }
}

impl From<BotanixChainSpec> for ChainSpec {
    fn from(value: BotanixChainSpec) -> Self {
        value.inner
    }
}
