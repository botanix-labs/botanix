use std::{fmt::Display, sync::Arc};

use alloy_consensus::transaction::{Recovered, Transaction};
use alloy_eips::eip7685::Requests;
use botanix_authority_edh::header_ext::HeaderExt;
use botanix_authority_peg::{
    consensus_package::BotanixConsensusPackage,
    mint_validation::{try_parse_burn_event, try_parse_mint_event, MintContractError},
    peg_contract::{PeginData, PegoutWithId},
};
use botanix_btc_wallet::bitcoind::BitcoindFactory;
use botanix_chainspec::BotanixChainSpec;
use btcserverlib::pegout_id::PegoutId;
use reth_chainspec::{ChainSpec, EthereumHardfork, EthereumHardforks};
use reth_evm::{
    revm::primitives::{alloy_primitives::BlockNumber, hex},
    ConfigureEvm, Database as RethDatabase, EvmEnvFor,
};
use reth_execution_errors::InternalBlockExecutionError;
use reth_node_types::NodePrimitives;
use reth_primitives::{Receipt, RecoveredBlock, SealedHeader};

use reth_primitives_traits::{AlloyBlockHeader, Block, BlockBody, SignedTransaction};
use reth_provider::{BlockExecutionResult, BlockReader, DatabaseProviderRO, ProviderError};
use reth_revm::{
    context::result::{ExecutionResult, ResultAndState},
    primitives::{alloy_primitives::TxHash, Address, U256},
    state::{Account, AccountStatus},
};
use revm_database::{
    states::bundle_state::{BundleRetention, BundleState},
    Database as RevmDatabase, DatabaseCommit, State,
};
use tracing::{error, info};

use crate::{
    error::{BlockExecutionError, BlockValidationError},
    state::post_block_balance_increments,
    utils::tx_type,
};

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

/// [`BlockExecutionResult`] combined with state and botanix-specific data.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    derive_more::AsRef,
    derive_more::AsMut,
    derive_more::Deref,
    derive_more::DerefMut,
)]
pub struct BotanixBlockExecutionOutput<T> {
    /// All the receipts of the transactions in the block.
    #[as_ref]
    #[as_mut]
    #[deref]
    #[deref_mut]
    pub result: BlockExecutionResult<T>,
    /// The changed state of the block after execution.
    pub state: BundleState,
    /// Total block fees
    pub total_block_fees: u128,
    /// Pegins
    pub pegins: Vec<PeginData>,
    /// Pegouts
    pub pegouts: Vec<PegoutWithId>,
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
    DB: RevmDatabase,
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

    /// Apply post execution state changes that do not require an [EVM](Evm), such as: block
    /// rewards, withdrawals, and irregular DAO hardfork state change
    pub fn post_execution(
        &mut self,
        total_block_fees: Option<u128>,
        block_fee_recipient_address: Address,
    ) -> Result<(), BlockExecutionError> {
        let balance_increments = post_block_balance_increments(
            self.botanix_chain_spec(),
            total_block_fees,
            Some(block_fee_recipient_address),
        );

        // increment balances
        self.state
            .increment_balances(balance_increments)
            .map_err(|_| BlockValidationError::IncrementBalanceFailed)?;

        Ok(())
    }
}

impl<EvmConfig, DB, BF, RethDB, N> EthBlockExecutor<EvmConfig, DB, BF, RethDB, N>
where
    EvmConfig: ConfigureEvm<Primitives = N::Primitives>,
    DB: RethDatabase<Error: Into<ProviderError> + Display>,
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
        N: reth_provider::providers::NodeTypesForProvider,
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
        self.post_execution(Some(output.total_block_fees), block_fee_recipient_address)?;

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
        E::DB: DatabaseCommit + RevmDatabase,
        E::Tx: reth_evm::FromRecoveredTx<<N::Primitives as NodePrimitives>::SignedTx>,
        <N::Primitives as NodePrimitives>::BlockHeader: HeaderExt,
        N: reth_provider::providers::NodeTypesForProvider,
    {
        // Apply pre execution changes
        let mut system_caller =
            reth_evm::system_calls::SystemCaller::new(self.botanix_chain_spec.inner());
        system_caller.apply_pre_execution_changes(block.header(), &mut evm)?;

        // execute transactions
        let mut total_pegins: Vec<PeginData> = vec![];
        let mut total_pegouts: Vec<PegoutWithId> = vec![];

        let mut total_block_fees = 0_u128;
        let mut cumulative_gas_used = 0;
        let base_fee = block.base_fee_per_gas();
        let mut receipts: Vec<Receipt> = Vec::with_capacity(block.body().transactions().len());

        for (tx_index, (sender, transaction)) in
            block.senders().iter().zip(block.body().transactions()).enumerate()
        {
            // The sum of the transaction’s gas limit, Tg, and the gas utilized in this block prior,
            // must be no greater than the block’s gasLimit.
            let block_available_gas = block.header().gas_limit() - cumulative_gas_used;
            if transaction.gas_limit() > block_available_gas {
                return Err(BlockValidationError::TransactionGasLimitMoreThanAvailableBlockGas {
                    transaction_gas_limit: transaction.gas_limit(),
                    block_available_gas,
                }
                .into());
            }

            // Store original sender account info before transaction execution.
            // This is used later to partially revert the state if botanix specific validation
            // fails.
            // If the sender is not found in the state, we need to error because there is no balance
            // to subtract from. This shouldn't happen because the tx would have failed.
            let sender_db_error =
                BlockExecutionError::Internal(InternalBlockExecutionError::Other(
                    format!("DB error getting sender: {}", hex::encode(sender)).into(),
                ));
            let sender_not_found =
                BlockExecutionError::Internal(InternalBlockExecutionError::Other(
                    format!("Sender not found in state: {}", hex::encode(sender)).into(),
                ));
            let mut original_sender_info: reth_revm::state::AccountInfo = evm
                .db_mut()
                .basic(*sender)
                .map_err(|_| sender_db_error)?
                .ok_or(sender_not_found)?;

            // Execute transaction.
            let recovered_tx: Recovered<<N::Primitives as NodePrimitives>::SignedTx> =
                Recovered::new_unchecked(transaction.clone(), *sender);
            let tx_hash = transaction.tx_hash();
            let ResultAndState { mut result, mut state } =
                evm.transact(recovered_tx).map_err(move |err| {
                    BlockExecutionError::Internal(InternalBlockExecutionError::Other(
                        format!("EVM error for transaction {}: {}", tx_hash, err).into(),
                    ))
                })?;

            // calculate the total transaction fee
            let mut transaction_fee = transaction
                .clone()
                .effective_tip_per_gas(base_fee.unwrap_or_default()) // TODO: is this right??
                .expect("base fee exists");
            // Include the base fee so it's not burned
            transaction_fee += base_fee.unwrap_or(0) as u128;
            total_block_fees += transaction_fee * u128::from(result.gas_used());

            // append gas used
            cumulative_gas_used += result.gas_used();

            // ***** Botanix specific checks ******
            let mut pegins = vec![];
            let mut pegouts = vec![];

            let new_result = {
                if result.is_success() {
                    match self.botanix_mint_contract_checks(
                        &result,
                        &botanix_consensus_pkg,
                        *transaction.tx_hash(),
                        provider.clone(),
                    ) {
                        Ok((new_pegins, new_pegouts)) => {
                            pegins.extend(new_pegins);
                            pegouts.extend(new_pegouts);
                            result
                        }
                        Err(e) => {
                            info!("Botanix Minting contract event validation failed: {:?}", e);

                            // Capture gas used from the initially successful execution
                            let gas_used = result.gas_used();

                            // Determine the total gas cost: gas_used * effective_gas_price
                            // Base fee is needed for effective_gas_price calculation
                            let effective_gas_price = transaction.effective_gas_price(base_fee);
                            let total_gas_cost =
                                U256::from(gas_used) * U256::from(effective_gas_price);

                            // Get the new nonce. This should be original nonce + 1
                            let new_nonce = state
                                .get(sender)
                                .ok_or(BlockExecutionError::Internal(
                                    InternalBlockExecutionError::Other(
                                        format!(
                                            "Sender not found in state: {}",
                                            hex::encode(sender)
                                        )
                                        .into(),
                                    ),
                                ))?
                                .info
                                .nonce;

                            // Clear ALL state changes introduced by the transaction.
                            // State is the diff of the previous state and the new state after the
                            // transaction.
                            state.clear();

                            // Now, re-apply *only* the total gas cost and nonce change to the
                            // pre-transaction state.
                            original_sender_info.nonce = new_nonce;
                            // There shouldn't be an underflow because the tx would have failed if
                            // the sender didn't have enough balance.
                            original_sender_info.balance = original_sender_info
                                .balance
                                .checked_sub(total_gas_cost)
                                .ok_or(BlockExecutionError::Internal(
                                    InternalBlockExecutionError::Other(
                                        "Sender balance underflow".to_string().into(),
                                    ),
                                ))?;

                            // Re-insert the sender's account with the original info but updated
                            // nonce and balance (reflecting only gas cost).
                            // Create new diff with only above changes.
                            let reverted_account = Account {
                                info: original_sender_info,
                                storage: Default::default(), // Storage changes remain reverted.
                                status: AccountStatus::Touched, // Mark as touched
                                transaction_id: tx_index,    /* TODO: confirm tx_index is correct
                                                              * here */
                            };
                            state.insert(*sender, reverted_account);

                            // Return a revert result, indicating failure but consuming gas and
                            // incrementing nonce.
                            ExecutionResult::Revert {
                                gas_used, // Still report the gas used
                                output: Default::default(),
                            }
                        }
                    }
                } else {
                    result
                }
            };

            result = new_result;

            evm.db_mut().commit(state);

            // Push transaction changeset and calculate header bloom filter for receipt.
            let tx_type = tx_type(transaction);

            receipts.push(
                #[allow(clippy::needless_update)] // side-effect of optimism fields
                Receipt {
                    tx_type,
                    // Success flag was added in `EIP-658: Embedding transaction status code in
                    // receipts`.
                    success: result.is_success(),
                    cumulative_gas_used,
                    // convert to reth log
                    logs: result.into_logs(),
                    ..Default::default()
                },
            );
            total_pegins.extend(pegins);
            total_pegouts.extend(pegouts);
        }

        Ok(EthExecuteOutput {
            receipts,
            requests: Requests::new(vec![]), // this is only used for ethereum validators
            gas_used: cumulative_gas_used,
            total_block_fees,
            pegins: total_pegins,
            pegouts: total_pegouts,
        })
    }

    /// Performs additional checks on mint contract transactions.
    #[tracing::instrument(
        level = "trace",
        skip(self, result, botanix_consensus_pkg, provider),
        fields(
            bitcoin_checkpoint_hash = %botanix_consensus_pkg.bitcoin_checkpoint.0.block_hash(),
            bitcoin_checkpoint_height = botanix_consensus_pkg.bitcoin_checkpoint.1,
        )
    )]
    fn botanix_mint_contract_checks<HR>(
        &self,
        result: &ExecutionResult<HR>,
        botanix_consensus_pkg: &BotanixConsensusPackage,
        tx_hash: TxHash,
        provider: Arc<DatabaseProviderRO<RethDB, N>>,
    ) -> Result<(Vec<PeginData>, Vec<PegoutWithId>), MintContractError>
    where
        N: reth_provider::providers::NodeTypesForProvider,
        <N::Primitives as NodePrimitives>::BlockHeader: HeaderExt,
    {
        let consensus_pkg = botanix_consensus_pkg;
        let btc_network = consensus_pkg.btc_network;

        tracing::trace!("botanix_consensus_package={:?}", botanix_consensus_pkg);

        // Check pegins.
        let mut pegins = vec![];
        let mut pegouts = vec![];
        for log in result.logs() {
            let pegin_data = match try_parse_mint_event(log)? {
                None => continue,
                Some(p) => p,
            };

            tracing::trace!(?pegin_data, "validate pegin data for tx {}", tx_hash);

            // Get the reference block hash from the pegin metadata.
            // This is used to avoid the growing list of headers in the pegin metadata
            // by using a bitcoin checkpoint that is close to the pegin block height.
            // The reference block hash is only provided for version v1.
            let mut bitcoin_checkpoint = consensus_pkg.bitcoin_checkpoint;
            let (version, ref_block_hash) = if let Some(meta) = pegin_data.meta.first() {
                match (meta.version(), meta.ref_block_hash()) {
                    (1, None) => {
                        return Err(MintContractError::InvalidPeginData {
                            error: "Reference block hash cannot be found".to_string(),
                            revert_address: pegin_data.account,
                            revert_amount: pegin_data.amount,
                        })
                    }
                    (1, Some(hash)) => {
                        match provider.find_block_by_hash(hash, reth_provider::BlockSource::Any) {
                            Ok(Some(block)) => {
                                let header = block.header();
                                let package = header
                                    .botanix_consensus_package(
                                        self.bitcoin_network,
                                        self.bitcoind_factory.clone(),
                                    )
                                    .map_err(|_| MintContractError::InvalidPeginData {
                                        error: "Failed to get botanix consensus package"
                                            .to_string(),
                                        revert_address: pegin_data.account,
                                        revert_amount: pegin_data.amount,
                                    })?;
                                bitcoin_checkpoint = package.bitcoin_checkpoint;

                                tracing::debug!(
                                    pegin_meta_version = meta.version(),
                                    ref_eth_block_hash = %hash,
                                    overridden_btc_checkpoint_hash = %bitcoin_checkpoint.0.block_hash(),
                                    overridden_btc_checkpoint_height = %bitcoin_checkpoint.1,
                                    "overridden bitcoin checkpoint for V1 pegin via ref_block_hash"
                                );
                            }
                            Ok(None) => {
                                return Err(MintContractError::InvalidPeginData {
                                    error: "No block found for reference block hash".to_string(),
                                    revert_address: pegin_data.account,
                                    revert_amount: pegin_data.amount,
                                })
                            }
                            Err(_) => panic!("Database error fetching reference block hash"),
                        };
                    }
                    (0, Some(_)) => {
                        return Err(MintContractError::InvalidPeginData {
                            error: "Not expecting reference block hash in proof version 0"
                                .to_string(),
                            revert_address: pegin_data.account,
                            revert_amount: pegin_data.amount,
                        })
                    }
                    _ => {}
                };
                (meta.version(), meta.ref_block_hash())
            } else {
                return Err(MintContractError::InvalidPeginData {
                    error: "No proofs found in pegin data".to_string(),
                    revert_address: pegin_data.account,
                    revert_amount: pegin_data.amount,
                });
            };

            for meta in &pegin_data.meta {
                if meta.version() != version {
                    return Err(MintContractError::InvalidPeginData {
                        error: "Proofs have mismatching versions".to_string(),
                        revert_address: pegin_data.account,
                        revert_amount: pegin_data.amount,
                    });
                }

                if meta.ref_block_hash() != ref_block_hash {
                    return Err(MintContractError::InvalidPeginData {
                        error: "Proofs have mismatching reference block hashes".to_string(),
                        revert_address: pegin_data.account,
                        revert_amount: pegin_data.amount,
                    });
                }
            }

            // the pegin height must be equal or less than the required block depth (checkpoint)
            if pegin_data.bitcoin_block_height > bitcoin_checkpoint.1 {
                return Err(MintContractError::InvalidPeginData {
                    error: format!(
                        "pegin height {} greater than checkpoint of {}",
                        pegin_data.bitcoin_block_height, bitcoin_checkpoint.1,
                    ),
                    revert_address: pegin_data.account,
                    revert_amount: pegin_data.amount,
                });
            }
            let aggregate_public_key = consensus_pkg.aggregate_public_key;
            match pegin_data.validate(&bitcoin_checkpoint, &aggregate_public_key) {
                Ok(aggregate_value) => {
                    if pegin_data.amount >= aggregate_value {
                        return Err(MintContractError::InvalidPeginData {
                            error: format!(
                                "pegin amount should be less than aggregate value: \
                                    pegin aggregate value: {}; pegin amount: {}",
                                aggregate_value, pegin_data.amount,
                            ),
                            revert_address: pegin_data.account,
                            revert_amount: pegin_data.amount,
                        });
                    }

                    tracing::debug!(validated_aggregate_value = %aggregate_value, "pegin data validation succeeded");
                }
                Err(e) => {
                    tracing::debug!(error = ?e, ?pegin_data, "pegin data validation failed: {e}");
                    return Err(MintContractError::InvalidPeginData {
                        error: format!("pegin validation failed: {}", e),
                        revert_address: pegin_data.account,
                        revert_amount: pegin_data.amount,
                    });
                }
            }

            pegins.push(pegin_data);
        }

        // Check pegouts
        for (index, log) in result.logs().iter().enumerate() {
            if let Some(pegout_data) = try_parse_burn_event(log, btc_network)? {
                let mut tx_hash_array = [0u8; 32];
                tx_hash_array.copy_from_slice(tx_hash.as_slice());
                let pegout_id = PegoutId::new(tx_hash_array, index as u32);
                let pegout_with_id = PegoutWithId { data: pegout_data, id: pegout_id };
                pegouts.push(pegout_with_id);
            }
        }

        Ok((pegins, pegouts))
    }
}

impl<EvmConfig, DB, BF, RethDB, N> EthBlockExecutor<EvmConfig, DB, BF, RethDB, N>
where
    EvmConfig: ConfigureEvm<Primitives = N::Primitives>,
    DB: RethDatabase<Error: Into<ProviderError> + Display>,
    RethDB: reth_db::Database,
    BF: BitcoindFactory + Clone + Unpin + 'static,
    N: reth_node_types::NodeTypes + reth_provider::providers::NodeTypesForProvider,
    <N::Primitives as NodePrimitives>::BlockHeader: HeaderExt,
    <N::Primitives as NodePrimitives>::Receipt: From<Receipt>,
{
    /// Executes the block and commits the changes to the internal state.
    ///
    /// Returns the receipts of the transactions in the block.
    ///
    /// Returns an error if the block could not be executed or failed verification.
    pub fn execute(
        mut self,
        block: &RecoveredBlock<<N::Primitives as NodePrimitives>::Block>,
    ) -> Result<
        BotanixBlockExecutionOutput<<N::Primitives as NodePrimitives>::Receipt>,
        BlockExecutionError,
    > {
        let EthExecuteOutput { receipts, requests, gas_used, total_block_fees, pegins, pegouts } =
            self.execute_without_verification(block)?;

        // TODO NOTE: we need to merge keep the reverts for the bundle retention
        self.state.merge_transitions(BundleRetention::Reverts);
        let state = self.state.take_bundle();

        Ok(BotanixBlockExecutionOutput {
            result: BlockExecutionResult {
                receipts: receipts.into_iter().map(|r| r.into()).collect(),
                requests,
                gas_used
            },
            state,
            total_block_fees,
            pegins,
            pegouts,
        })
    }
}
