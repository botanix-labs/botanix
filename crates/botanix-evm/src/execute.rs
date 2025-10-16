use std::{fmt::Display, sync::Arc};

use alloy_eips::eip7685::Requests;
use botanix_authority_edh::header_ext::HeaderExt;
use botanix_authority_peg::{
    consensus_package::BotanixConsensusPackage,
    peg_contract::{PeginData, PegoutWithId},
};
use botanix_btc_wallet::bitcoind::BitcoindFactory;
use botanix_chainspec::BotanixChainSpec;
use reth_chainspec::{ChainSpec, EthereumHardforks};
use reth_evm::{
    execute::Executor,
    revm::primitives::{alloy_primitives::BlockNumber, U256},
    ConfigureEvm, Database, Evm, EvmEnvFor,
};
use reth_node_types::NodePrimitives;
use reth_primitives::{Header, Receipt, RecoveredBlock, SealedHeader};

use reth_primitives_traits::AlloyBlockHeader;
use reth_provider::{BlockExecutionOutput, DatabaseProviderRO, ProviderError};
use revm_database::{DatabaseCommit, State};
use tracing::error;

use crate::error::{BlockExecutionError, BlockValidationError};

/// Helper type for the output of executing a block.
#[derive(Debug, Clone)]
struct EthExecuteOutput {
    receipts: Vec<Receipt>,
    requests: Requests,
    gas_used: u64,
    total_block_fees: u128,
    pegins: Vec<PeginData>,
    pegouts: Vec<PegoutWithId>,
}

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

impl<EvmConfig, DB, BF, RethDB, N> EthBlockExecutor<EvmConfig, DB, BF, RethDB, N>
where
    EvmConfig: ConfigureEvm<Primitives = N::Primitives>,
    DB: Database<Error: Into<ProviderError> + Display>,
    BF: BitcoindFactory + Clone + Unpin + 'static,
    RethDB: reth_db::Database,
    N: reth_node_types::NodeTypes,
{
    /// Execute a single block and apply the state changes to the internal state.
    ///
    /// Returns the receipts of the transactions in the block, the total gas used and the list of
    /// EIP-7685 [requests](Request).
    ///
    /// Returns an error if execution fails.
    fn execute_without_verification(
        &mut self,
        block: &RecoveredBlock<<N::Primitives as NodePrimitives>::Block>,
    ) -> Result<EthExecuteOutput, BlockExecutionError>
    where
        <N::Primitives as NodePrimitives>::BlockHeader: HeaderExt,
    {
        // 1. prepare state on new block
        self.on_new_block(block.number());

        let header: &SealedHeader<<N::Primitives as NodePrimitives>::BlockHeader> =
            block.sealed_header();
        let edh = header.deserialize_extra_data_header().map_err(|_| {
            BlockExecutionError::Validation(BlockValidationError::ExtraDataSerializeError)
        })?;

        let botanix_consensus_pkg = header
            .botanix_consensus_package(
                self.executor.bitcoin_network,
                self.executor.bitcoind_factory.clone(),
            )
            .map_err(|e| {
                error!("Failed to get botanix consensus package: {:?}", e);
                BlockExecutionError::Validation(BlockValidationError::BotanixConsensusPkgError(e))
            })?;

        let block_fee_recipient_address = edh.block_fee_recipient_address;

        // 2. configure the evm and execute
        let env = self.evm_env_for_block(block.header());
        let output: EthExecuteOutput = {
            let evm = self.executor.evm_config.evm_with_env(&mut self.state, env);
            self.executor.execute_state_transitions(
                block,
                evm,
                botanix_consensus_pkg,
                self.executor.provider.clone(),
            )?
        };

        // 3. apply post execution changes
        self.post_execution(
            block,
            total_difficulty,
            Some(output.total_block_fees),
            block_fee_recipient_address,
        )?;

        Ok(output)
    }

    /// Apply settings before a new block is executed.
    pub(crate) fn on_new_block(&mut self, block_number: BlockNumber) {
        // Set state clear flag if the block is after the Spurious Dragon hardfork.
        let state_clear_flag =
            self.botanix_chain_spec().is_spurious_dragon_active_at_block(block_number);
        self.state.set_state_clear_flag(state_clear_flag);
    }

    /// Configures a new evm configuration and block environment for the given block.
    ///
    /// # Caution
    ///
    /// This does not initialize the tx environment.
    fn evm_env_for_block(
        &self,
        header: &<EvmConfig::Primitives as NodePrimitives>::BlockHeader,
    ) -> EvmEnvFor<EvmConfig>
    where
        EvmConfig: ConfigureEvm,
    {
        // let mut cfg = CfgEnvWithHandlerCfg::new(Default::default(), Default::default());
        // let mut block_env = BlockEnv::default();
        // self.executor.evm_config.fill_cfg_and_block_env(
        //     &mut cfg,
        //     &mut block_env,
        //     self.chain_spec(),
        //     header,
        //     total_difficulty,
        // );

        // EnvWithHandlerCfg::new_with_cfg_env(cfg, block_env, Default::default())

        self.executor.evm_config.evm_env(header)
    }
}

impl<EvmConfig, BF, RethDB, N> EthEvmExecutor<EvmConfig, BF, RethDB, N>
where
    EvmConfig: ConfigureEvm<Primitives = N::Primitives>,
    BF: BitcoindFactory + Clone + Unpin + 'static,
    RethDB: reth_db::Database,
    N: reth_node_types::NodeTypes,
{
    /// Executes the transactions in the block and returns the receipts of the transactions in the
    /// block, the total gas used and the list of EIP-7685 [requests](Request).
    /// As well as pegins and pegouts
    ///
    /// This applies the pre-execution and post-execution changes that require an [EVM](Evm), and
    /// executes the transactions.
    ///
    /// # Note
    ///
    /// It does __not__ apply post-execution changes that do not require an [EVM](Evm), for that see
    /// [`EthBlockExecutor::post_execution`].
    fn execute_state_transitions<E>(
        &self,
        block: &RecoveredBlock<<N::Primitives as NodePrimitives>::Block>,
        mut evm: E,
        botanix_consensus_pkg: BotanixConsensusPackage,
        provider: Arc<DatabaseProviderRO<RethDB, N>>,
    ) -> Result<EthExecuteOutput, BlockExecutionError>
    where
        E: reth_evm::Evm,
        E::DB: DatabaseCommit,
    {
        // Apply pre execution changes
        let mut system_caller =
            reth_evm::system_calls::SystemCaller::new(self.botanix_chain_spec.inner());
        system_caller.apply_pre_execution_changes(block.header(), &mut evm)?;

        // TODO: execute transactions and handle botanix-specific logic

        todo!("Complete execute_state_transitions implementation")
    }
}

impl<EvmConfig, DB, BF, RethDB, N> Executor<DB> for EthBlockExecutor<EvmConfig, DB, BF, RethDB, N>
where
    EvmConfig: ConfigureEvm<Primitives = N::Primitives>,
    DB: Database<Error: Into<ProviderError> + Display>,
    RethDB: reth_db::Database,
    BF: BitcoindFactory + Clone + Unpin + 'static,
    N: reth_node_types::NodeTypes,
    <N::Primitives as NodePrimitives>::BlockHeader: HeaderExt,
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
        let EthExecuteOutput { receipts, requests, gas_used, total_block_fees, pegins, pegouts } =
            self.execute_without_verification(block)?;

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
