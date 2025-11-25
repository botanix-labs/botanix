use super::patch::{
    patch_chapel_after_tx, patch_chapel_before_tx, patch_mainnet_after_tx,
    patch_mainnet_before_tx,
};
use crate::evm::transaction::BotanixTxEnv;
use alloy_consensus::{constants::ETH_TO_WEI, Transaction, TxReceipt};
use alloy_eips::{
    eip2935::{HISTORY_STORAGE_ADDRESS, HISTORY_STORAGE_CODE},
    eip7685::Requests,
    Encodable2718,
};
use alloy_evm::{
    block::{ExecutableTx, StateChangeSource},
    eth::receipt_builder::ReceiptBuilderCtx,
};
use alloy_primitives::{
    address, keccak256, uint, Address, BlockNumber, Bytes, TxKind, U256,
};
use alloy_sol_macro::sol;
use alloy_sol_types::SolCall;
use botanix_chainspec::BotanixHardforks;
use reth_chainspec::{EthChainSpec, EthereumHardforks, Hardforks};
use reth_evm::{
    block::{BlockValidationError, CommitChanges},
    eth::{receipt_builder::ReceiptBuilder, EthBlockExecutionCtx},
    execute::{BlockExecutionError, BlockExecutor},
    system_calls::SystemCaller,
    Database, Evm, FromRecoveredTx, FromTxWithEncoded, IntoTxEnv, OnStateHook,
    RecoveredTx,
};
use reth_primitives::TransactionSigned;
use reth_primitives_traits::SignerRecoverable;
use reth_provider::BlockExecutionResult;
use reth_revm::State;
use revm::{
    context::{
        result::{ExecutionResult, ResultAndState},
        TxEnv,
    },
    state::Bytecode,
    Database as _, DatabaseCommit,
};
use tracing::debug;

// TODO: Determine if and how this is being used.
pub(super) struct BotanixBlockExecutor<'a, EVM, Spec, R: ReceiptBuilder>
where
    Spec: EthChainSpec,
{
    /// Reference to the specification object.
    spec: Spec,
    /// Inner EVM.
    evm: EVM,
    /// Gas used in the block.
    gas_used: u64,
    /// Receipts of executed transactions.
    receipts: Vec<R::Receipt>,
    /// System txs
    system_txs: Vec<R::Transaction>,
    /// Receipt builder.
    receipt_builder: R,
    /// Context for block execution.
    _ctx: EthBlockExecutionCtx<'a>,
    /// Utility to call system caller.
    system_caller: SystemCaller<Spec>,
}

impl<'a, DB, EVM, Spec, R: ReceiptBuilder>
    BotanixBlockExecutor<'a, EVM, Spec, R>
where
    DB: Database + 'a,
    EVM: Evm<
        DB = &'a mut State<DB>,
        Tx: FromRecoveredTx<R::Transaction>
                + FromRecoveredTx<TransactionSigned>
                + FromTxWithEncoded<TransactionSigned>,
    >,
    Spec:
        EthereumHardforks + BotanixHardforks + EthChainSpec + Hardforks + Clone,
    R: ReceiptBuilder<Transaction = TransactionSigned, Receipt: TxReceipt>,
    <R as ReceiptBuilder>::Transaction: Unpin + From<TransactionSigned>,
    <EVM as alloy_evm::Evm>::Tx:
        FromTxWithEncoded<<R as ReceiptBuilder>::Transaction>,
    BotanixTxEnv: IntoTxEnv<<EVM as alloy_evm::Evm>::Tx>,
    R::Transaction: Into<TransactionSigned>,
{
    /// Creates a new BotanixBlockExecutor.
    pub(super) fn new(
        evm: EVM,
        _ctx: EthBlockExecutionCtx<'a>,
        spec: Spec,
        receipt_builder: R,
    ) -> Self {
        let spec_clone = spec.clone();
        Self {
            spec,
            evm,
            gas_used: 0,
            receipts: vec![],
            system_txs: vec![],
            receipt_builder,
            _ctx,
            system_caller: SystemCaller::new(spec_clone),
        }
    }

    pub(crate) fn apply_history_storage_account(
        &mut self,
        block_number: BlockNumber,
    ) -> Result<bool, BlockExecutionError> {
        debug!(
            "Apply history storage account {:?} at height {:?}",
            HISTORY_STORAGE_ADDRESS, block_number
        );

        let account = self
            .evm
            .db_mut()
            .load_cache_account(HISTORY_STORAGE_ADDRESS)
            .map_err(BlockExecutionError::other)?;

        let mut new_info = account.account_info().unwrap_or_default();
        new_info.code_hash = keccak256(HISTORY_STORAGE_CODE.clone());
        new_info.code =
            Some(Bytecode::new_raw(Bytes::from_static(&HISTORY_STORAGE_CODE)));
        new_info.nonce = 1_u64;
        new_info.balance = U256::ZERO;

        let transition = account.change(new_info, Default::default());
        self.evm
            .db_mut()
            .apply_transition(vec![(HISTORY_STORAGE_ADDRESS, transition)]);
        Ok(true)
    }
}

impl<'a, DB, E, Spec, R> BlockExecutor for BotanixBlockExecutor<'a, E, Spec, R>
where
    DB: Database + 'a,
    E: Evm<
        DB = &'a mut State<DB>,
        Tx: FromRecoveredTx<R::Transaction>
                + FromRecoveredTx<TransactionSigned>
                + FromTxWithEncoded<TransactionSigned>,
    >,
    Spec: EthereumHardforks + BotanixHardforks + EthChainSpec + Hardforks,
    R: ReceiptBuilder<Transaction = TransactionSigned, Receipt: TxReceipt>,
    <R as ReceiptBuilder>::Transaction: Unpin + From<TransactionSigned>,
    <E as alloy_evm::Evm>::Tx:
        FromTxWithEncoded<<R as ReceiptBuilder>::Transaction>,
    BotanixTxEnv: IntoTxEnv<<E as alloy_evm::Evm>::Tx>,
    R::Transaction: Into<TransactionSigned>,
{
    type Transaction = TransactionSigned;
    type Receipt = R::Receipt;
    type Evm = E;

    // This method isn't currently used.
    fn apply_pre_execution_changes(
        &mut self,
    ) -> Result<(), BlockExecutionError> {
        // Set state clear flag if the block is after the Spurious Dragon
        // hardfork.
        let state_clear_flag = self
            .spec
            .is_spurious_dragon_active_at_block(self.evm.block().number.to());
        self.evm.db_mut().set_state_clear_flag(state_clear_flag);

        if !self
            .spec
            .is_pectra_active_at_timestamp(self.evm.block().timestamp.to())
        {
            // This should never happen as Botanix always has Pectra active
            panic!("Pectra hardfork not active at timestamp!!",);
        }

        // enable historical block hashes from state
        if self.spec.is_pectra_transition_at_timestamp(
            self.evm.block().timestamp.to(),
            self.evm.block().timestamp.to::<u64>() - 3,
        ) {
            self.apply_history_storage_account(
                self.evm.block().number.to::<u64>(),
            )?;
        }
        if self
            .spec
            .is_prague_active_at_timestamp(self.evm.block().timestamp.to())
        {
            self.system_caller.apply_blockhashes_contract_call(
                self._ctx.parent_hash,
                &mut self.evm,
            )?;
        }

        Ok(())
    }

    // Noop
    fn execute_transaction_with_commit_condition(
        &mut self,
        _tx: impl ExecutableTx<Self>,
        _f: impl FnOnce(
            &ExecutionResult<<Self::Evm as Evm>::HaltReason>,
        ) -> CommitChanges,
    ) -> Result<Option<u64>, BlockExecutionError> {
        Ok(Some(0))
    }

    // Noop
    fn execute_transaction_with_result_closure(
        &mut self,
        tx: impl ExecutableTx<Self>
            + IntoTxEnv<<E as alloy_evm::Evm>::Tx>
            + RecoveredTx<TransactionSigned>,
        f: impl for<'b> FnOnce(
            &'b ExecutionResult<<E as alloy_evm::Evm>::HaltReason>,
        ),
    ) -> Result<u64, BlockExecutionError> {
        Ok(0)
    }

    // This is basically a noop since we don't have any special system tx
    // handling after execution
    fn finish(
        mut self,
    ) -> Result<
        (Self::Evm, BlockExecutionResult<R::Receipt>),
        BlockExecutionError,
    > {
        Ok((
            self.evm,
            BlockExecutionResult {
                receipts: self.receipts,
                requests: Requests::default(),
                gas_used: self.gas_used,
            },
        ))
    }

    fn set_state_hook(&mut self, _hook: Option<Box<dyn OnStateHook>>) {
        self.system_caller.with_state_hook(_hook);
    }

    fn evm_mut(&mut self) -> &mut Self::Evm {
        &mut self.evm
    }

    fn evm(&self) -> &Self::Evm {
        &self.evm
    }
}
