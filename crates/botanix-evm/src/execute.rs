use std::sync::Arc;

use botanix_chainspec::BotanixChainSpec;
use reth_provider::DatabaseProviderRO;

/// Helper container type for EVM with chain spec.
#[derive(Debug, Clone)]
struct EthEvmExecutor<EvmConfig, BF, RethDB, N>
where
    RethDB: reth_db::Database,
{
    /// Botanix chainspec
    botanix_chain_spec: Arc<BotanixChainSpec>,
    /// How to create an EVM.
    evm_config: EvmConfig,
    /// The bitcoind factory used to connect to the L1 bitcoind RPC
    bitcoind_factory: BF,
    /// The L1 bitcoin network
    bitcoin_network: bitcoin::Network,
    /// Blockchain provider
    provider: Arc<DatabaseProviderRO<RethDB, N>>,
}

/// A basic Ethereum block executor.
///
/// Expected usage:
/// - Create a new instance of the executor.
/// - Execute the block.
#[derive(Debug)]
pub struct EthBlockExecutor<EvmConfig, DB, BF, RethDB>
where
    RethDB: reth_db::Database,
{
    /// Chain specific evm config that's used to execute a block.
    executor: EthEvmExecutor<EvmConfig, BF, RethDB>,
    /// The state to use for execution
    state: State<DB>,
}

impl<EvmConfig, DB, BF, RethDB> EthBlockExecutor<EvmConfig, DB, BF, RethDB>
where
    RethDB: reth_db::Database,
{
    /// Creates a new Ethereum block executor.
    pub const fn new(
        botanix_chain_spec: Arc<BotanixChainSpec>,
        evm_config: EvmConfig,
        state: State<DB>,
        bitcoind_factory: BF,
        bitcoin_network: bitcoin::Network,
        provider: Arc<DatabaseProviderRO<RethDB>>,
    ) -> Self {
        Self {
            executor: EthEvmExecutor {
                botanix_chain_spec,
                evm_config,
                bitcoind_factory,
                bitcoin_network,
                provider,
            },
            state,
        }
    }

    #[inline]
    fn chain_spec(&self) -> &ChainSpec {
        self.executor.botanix_chain_spec.inner()
    }

    #[inline]
    fn botanix_chain_spec(&self) -> &BotanixChainSpec {
        self.executor.botanix_chain_spec.as_ref()
    }

    /// Returns mutable reference to the state that wraps the underlying database.
    #[allow(unused)]
    fn state_mut(&mut self) -> &mut State<DB> {
        &mut self.state
    }
}
