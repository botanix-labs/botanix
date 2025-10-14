use std::sync::Arc;

use botanix_btc_wallet::bitcoind::BitcoindFactory;
use botanix_chainspec::BotanixChainSpec;
use reth_chainspec::ChainSpec;
use reth_evm::execute::Executor;
use reth_node_types::NodePrimitives;
use reth_primitives::RecoveredBlock;
use reth_provider::{BlockExecutionOutput, DatabaseProviderRO};
use revm_database::State;

use crate::error::BlockExecutionError;

/// Helper container type for EVM with chain spec.
#[derive(Debug, Clone)]
struct EthEvmExecutor<EvmConfig, BF, RethDB, N>
where
    RethDB: reth_db::Database,
    N: reth_node_types::NodeTypes,
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
pub struct EthBlockExecutor<EvmConfig, DB, BF, RethDB, N>
where
    RethDB: reth_db::Database,
    N: reth_node_types::NodeTypes,
{
    /// Chain specific evm config that's used to execute a block.
    executor: EthEvmExecutor<EvmConfig, BF, RethDB, N>,
    /// The state to use for execution
    state: State<DB>,
}

impl<EvmConfig, DB, BF, RethDB, N> EthBlockExecutor<EvmConfig, DB, BF, RethDB, N>
where
    RethDB: reth_db::Database,
    N: reth_node_types::NodeTypes,
{
    /// Creates a new Ethereum block executor.
    pub const fn new(
        botanix_chain_spec: Arc<BotanixChainSpec>,
        evm_config: EvmConfig,
        state: State<DB>,
        bitcoind_factory: BF,
        bitcoin_network: bitcoin::Network,
        provider: Arc<DatabaseProviderRO<RethDB, N>>,
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

impl<EvmConfig, DB, BF, RethDB, N> Executor<DB> for EthBlockExecutor<EvmConfig, DB, BF, RethDB, N>
where
    DB: reth_db::Database + revm_database::Database<Error: Sync + Send + 'static>,
    RethDB: reth_db::Database,
    BF: BitcoindFactory + Clone + Unpin + 'static,
    N: reth_node_types::NodeTypes,
{
    type Error = BlockExecutionError;

    type Primitives = N::Primitives;

    /// Executes the block and commits the changes to the internal state.
    ///
    /// Returns the receipts of the transactions in the block.
    ///
    /// Returns an error if the block could not be executed or failed verification.
    fn execute(
        mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
    ) -> Result<BlockExecutionOutput<<Self::Primitives as NodePrimitives>::Receipt>, Self::Error>
    {
        let BlockExecutionInput { block, total_difficulty } = block.sealed_block();
        let EthExecuteOutput { receipts, requests, gas_used, total_block_fees, pegins, pegouts } =
            self.execute_without_verification(block, total_difficulty)?;

        // TODO NOTE: we need to merge keep the reverts for the bundle retention
        self.state.merge_transitions(BundleRetention::Reverts);
        Ok(BlockExecutionOutput {
            state: self.state.take_bundle(),
            receipts,
            requests,
            gas_used,
            total_block_fees,
            pegins,
            pegouts,
        })
    }

    fn execute_one(
        &mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
    ) -> Result<
        reth_provider::BlockExecutionResult<<Self::Primitives as NodePrimitives>::Receipt>,
        Self::Error,
    > {
        todo!()
    }

    fn execute_one_with_state_hook<F>(
        &mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
        state_hook: F,
    ) -> Result<
        reth_provider::BlockExecutionResult<<Self::Primitives as NodePrimitives>::Receipt>,
        Self::Error,
    >
    where
        F: reth_evm::OnStateHook + 'static,
    {
        todo!()
    }

    fn into_state(self) -> State<DB> {
        todo!()
    }

    fn size_hint(&self) -> usize {
        todo!()
    }

    fn execute_batch<'a, I>(
        mut self,
        blocks: I,
    ) -> Result<
        reth_provider::ExecutionOutcome<<Self::Primitives as NodePrimitives>::Receipt>,
        Self::Error,
    >
    where
        I: IntoIterator<Item = &'a RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>>,
    {
        let mut results = Vec::new();
        let mut first_block = None;
        for block in blocks {
            if first_block.is_none() {
                first_block = Some(block.header().number());
            }
            results.push(self.execute_one(block)?);
        }

        Ok(reth_provider::ExecutionOutcome::from_blocks(
            first_block.unwrap_or_default(),
            self.into_state().take_bundle(),
            results,
        ))
    }

    fn execute_with_state_closure<F>(
        mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
        mut f: F,
    ) -> Result<
        reth_provider::BlockExecutionOutput<<Self::Primitives as NodePrimitives>::Receipt>,
        Self::Error,
    >
    where
        F: FnMut(&State<DB>),
    {
        let result = self.execute_one(block)?;
        let mut state = self.into_state();
        f(&state);
        Ok(reth_provider::BlockExecutionOutput { state: state.take_bundle(), result })
    }

    fn execute_with_state_hook<F>(
        mut self,
        block: &RecoveredBlock<<Self::Primitives as NodePrimitives>::Block>,
        state_hook: F,
    ) -> Result<
        reth_provider::BlockExecutionOutput<<Self::Primitives as NodePrimitives>::Receipt>,
        Self::Error,
    >
    where
        F: reth_evm::OnStateHook + 'static,
    {
        let result = self.execute_one_with_state_hook(block, state_hook)?;
        let mut state = self.into_state();
        Ok(reth_provider::BlockExecutionOutput { state: state.take_bundle(), result })
    }
}
