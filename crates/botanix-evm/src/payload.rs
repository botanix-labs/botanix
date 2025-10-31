use std::sync::Arc;

use alloy_consensus::Transaction;
use reth_basic_payload_builder::{is_better_payload, BuildArguments, BuildOutcome, PayloadConfig};
use reth_chainspec::{EthChainSpec, EthereumHardforks};
use reth_ethereum_engine_primitives::{EthBuiltPayload, EthPayloadBuilderAttributes};
use reth_ethereum_payload_builder::EthereumBuilderConfig;
use reth_evm::{
    execute::{BlockBuilder, BlockBuilderOutcome},
    ConfigureEvm, Evm, NextBlockEnvAttributes,
};
use reth_execution_errors::{BlockExecutionError, BlockValidationError};
use reth_payload_builder::BlobSidecars;
use reth_payload_primitives::{PayloadBuilderAttributes, PayloadBuilderError};
use reth_primitives::{InvalidTransactionError, NodePrimitives, TransactionSigned};
use reth_primitives_traits::BlockBody as BlockBodyTrait;
use reth_provider::{ChainSpecProvider, StateProviderFactory};
use reth_revm::{context::Block, database::StateProviderDatabase, primitives::U256, State};
use reth_transaction_pool::{
    error::{Eip4844PoolTransactionError, InvalidPoolTransactionError},
    BestTransactions, BestTransactionsAttributes, PoolTransaction, TransactionPool,
    ValidPoolTransaction,
};
use tracing::{debug, trace, warn};

// Copied from Reth since it's private
type BestTransactionsIter<Pool> = Box<
    dyn BestTransactions<Item = Arc<ValidPoolTransaction<<Pool as TransactionPool>::Transaction>>>,
>;

/// Constructs an Ethereum transaction payload using the best transactions from the pool.
///
/// Given build arguments including an Ethereum client, transaction pool,
/// and configuration, this function creates a transaction payload. Returns
/// a result indicating success with the payload or an error in case of failure.
/// This is mainly a copy and paste from Reth with the additional functionality of passing a floor
/// base fee to use when calculating the base fee.
#[inline]
pub fn default_ethereum_payload<Primitives, EvmConfig, Client, Pool, F>(
    evm_config: EvmConfig,
    client: Client,
    pool: Pool,
    builder_config: EthereumBuilderConfig,
    args: BuildArguments<EthPayloadBuilderAttributes, EthBuiltPayload>,
    best_txs: F,
    floor_base_fee: Option<u64>,
) -> Result<BuildOutcome<EthBuiltPayload>, PayloadBuilderError>
where
    Primitives: NodePrimitives<BlockHeader = alloy_consensus::Header, SignedTx = TransactionSigned>,
    EvmConfig: ConfigureEvm<Primitives = Primitives, NextBlockEnvCtx = NextBlockEnvAttributes>,
    Client: StateProviderFactory + ChainSpecProvider<ChainSpec: EthereumHardforks>,
    Pool: TransactionPool<Transaction: PoolTransaction<Consensus = TransactionSigned>>,
    F: FnOnce(BestTransactionsAttributes) -> BestTransactionsIter<Pool>,
{
    let BuildArguments { mut cached_reads, config, cancel, best_payload, max_tx_bytes: _ } = args;
    let PayloadConfig { parent_header, attributes } = config;

    let state_provider = client.state_by_block_hash(parent_header.hash())?;
    let state = StateProviderDatabase::new(&state_provider);
    let mut db =
        State::builder().with_database(cached_reads.as_db_mut(state)).with_bundle_update().build();

    let mut builder = evm_config
        .builder_for_next_block(
            &mut db,
            &parent_header,
            NextBlockEnvAttributes {
                timestamp: attributes.timestamp(),
                suggested_fee_recipient: attributes.suggested_fee_recipient(),
                prev_randao: attributes.prev_randao(),
                gas_limit: builder_config.gas_limit(parent_header.gas_limit),
                parent_beacon_block_root: attributes.parent_beacon_block_root(),
                withdrawals: Some(attributes.withdrawals().clone()),
            },
        )
        .map_err(PayloadBuilderError::other)?;

    let chain_spec = client.chain_spec();

    debug!(target: "payload_builder", id=%attributes.id, parent_header = ?parent_header.hash(), parent_number = parent_header.number, "building new payload");
    let mut cumulative_gas_used = 0;
    let block_gas_limit: u64 = builder.evm_mut().block().gas_limit;
    // Set the base fee using the floor_base_fee
    let base_fee = {
        let floor_base_fee = floor_base_fee.unwrap_or(0);
        let base_fee = builder.evm_mut().block().basefee;
        base_fee.max(floor_base_fee)
    };

    let mut best_txs = best_txs(BestTransactionsAttributes::new(
        base_fee,
        builder.evm_mut().block().blob_gasprice().map(|gasprice| gasprice as u64),
    ));
    let mut total_fees = U256::ZERO;

    builder.apply_pre_execution_changes().map_err(|err| {
        warn!(target: "payload_builder", %err, "failed to apply pre-execution changes");
        PayloadBuilderError::Internal(err.into())
    })?;

    // initialize empty blob sidecars at first. If cancun is active then this will be populated by
    // blob sidecars if any.
    let mut blob_sidecars = BlobSidecars::Empty;

    let mut block_blob_count = 0;

    let blob_params = chain_spec.blob_params_at_timestamp(attributes.timestamp);
    let max_blob_count =
        blob_params.as_ref().map(|params| params.max_blob_count).unwrap_or_default();

    while let Some(pool_tx) = best_txs.next() {
        // ensure we still have capacity for this transaction
        if cumulative_gas_used + pool_tx.gas_limit() > block_gas_limit {
            // we can't fit this transaction into the block, so we need to mark it as invalid
            // which also removes all dependent transaction from the iterator before we can
            // continue
            best_txs.mark_invalid(
                &pool_tx,
                InvalidPoolTransactionError::ExceedsGasLimit(pool_tx.gas_limit(), block_gas_limit),
            );
            continue
        }

        // check if the job was cancelled, if so we can exit early
        if cancel.is_cancelled() {
            return Ok(BuildOutcome::Cancelled)
        }

        // convert tx to a signed transaction
        let tx = pool_tx.to_consensus();

        // There's only limited amount of blob space available per block, so we need to check if
        // the EIP-4844 can still fit in the block
        let mut blob_tx_sidecar = None;
        if let Some(blob_tx) = tx.as_eip4844() {
            let tx_blob_count = blob_tx.tx().blob_versioned_hashes.len() as u64;

            if block_blob_count + tx_blob_count > max_blob_count {
                // we can't fit this _blob_ transaction into the block, so we mark it as
                // invalid, which removes its dependent transactions from
                // the iterator. This is similar to the gas limit condition
                // for regular transactions above.
                trace!(target: "payload_builder", tx=?tx.hash(), ?block_blob_count, "skipping
blob transaction because it would exceed the max blob count per block");
                best_txs.mark_invalid(
                    &pool_tx,
                    InvalidPoolTransactionError::Eip4844(
                        Eip4844PoolTransactionError::TooManyEip4844Blobs {
                            have: block_blob_count + tx_blob_count,
                            permitted: max_blob_count,
                        },
                    ),
                );
                continue
            }

            let blob_sidecar_result = 'sidecar: {
                let Some(sidecar) =
                    pool.get_blob(*tx.hash()).map_err(PayloadBuilderError::other)?
                else {
                    break 'sidecar Err(Eip4844PoolTransactionError::MissingEip4844BlobSidecar)
                };

                if chain_spec.is_osaka_active_at_timestamp(attributes.timestamp) {
                    if sidecar.is_eip7594() {
                        Ok(sidecar)
                    } else {
                        Err(Eip4844PoolTransactionError::UnexpectedEip4844SidecarAfterOsaka)
                    }
                } else if sidecar.is_eip4844() {
                    Ok(sidecar)
                } else {
                    Err(Eip4844PoolTransactionError::UnexpectedEip7594SidecarBeforeOsaka)
                }
            };

            blob_tx_sidecar = match blob_sidecar_result {
                Ok(sidecar) => Some(sidecar),
                Err(error) => {
                    best_txs.mark_invalid(&pool_tx, InvalidPoolTransactionError::Eip4844(error));
                    continue
                }
            };
        }

        let gas_used = match builder.execute_transaction(tx.clone()) {
            Ok(gas_used) => gas_used,
            Err(BlockExecutionError::Validation(BlockValidationError::InvalidTx {
                error, ..
            })) => {
                if error.is_nonce_too_low() {
                    // if the nonce is too low, we can skip this transaction
                    trace!(target: "payload_builder", %error, ?tx, "skipping nonce too lowtransaction");
                } else {
                    // if the transaction is invalid, we can skip it and all of its
                    // descendants
                    trace!(target: "payload_builder", %error, ?tx, "skipping invalid transaction and its descendants");
                    best_txs.mark_invalid(
                        &pool_tx,
                        InvalidPoolTransactionError::Consensus(
                            InvalidTransactionError::TxTypeNotSupported,
                        ),
                    );
                }
                continue
            }
            // this is an error that we should treat as fatal for this attempt
            Err(err) => return Err(PayloadBuilderError::evm(err)),
        };

        // add to the total blob gas used if the transaction successfully executed
        if let Some(blob_tx) = tx.as_eip4844() {
            block_blob_count += blob_tx.tx().blob_versioned_hashes.len() as u64;

            // if we've reached the max blob count, we can skip blob txs entirely
            if block_blob_count == max_blob_count {
                best_txs.skip_blobs();
            }
        }

        // update and add to total fees
        let miner_fee =
            tx.effective_tip_per_gas(base_fee).expect("fee is always valid; execution succeeded");
        total_fees += U256::from(miner_fee) * U256::from(gas_used);
        cumulative_gas_used += gas_used;

        // Add blob tx sidecar to the payload.
        if let Some(sidecar) = blob_tx_sidecar {
            blob_sidecars.push_sidecar_variant(sidecar.as_ref().clone());
        }
    }

    // check if we have a better block
    if !is_better_payload(best_payload.as_ref(), total_fees) {
        // Release db
        drop(builder);
        // can skip building the block
        return Ok(BuildOutcome::Aborted { fees: total_fees, cached_reads })
    }

    let BlockBuilderOutcome { execution_result, block, .. } = builder.finish(&state_provider)?;

    let requests = chain_spec
        .is_prague_active_at_timestamp(attributes.timestamp)
        .then_some(execution_result.requests);

    let sealed_block = block.sealed_block().clone();
    debug!(target: "payload_builder", id=%attributes.id, sealed_block_header = ?sealed_block.sealed_header(), "sealed built block");

    // Convert block to Block<EthereumTxEnvelope> to create the EthBuiltPayload
    let eth_block = {
        let header = sealed_block.header().clone();
        let body = sealed_block.body();

        // Create an Ethereum BlockBody from the generic body
        let transactions = body.transactions().to_vec();
        let ommers = body.ommers().unwrap_or_default().to_vec();
        let withdrawals = body.withdrawals().cloned();

        let eth_body = alloy_consensus::BlockBody { transactions, ommers, withdrawals };

        let eth_block = alloy_consensus::Block { header, body: eth_body };

        let block_hash = sealed_block.hash();
        reth_primitives_traits::SealedBlock::new_unchecked(eth_block, block_hash)
    };

    let payload = EthBuiltPayload::new(attributes.id, Arc::new(eth_block), total_fees, requests)
        // add blob sidecars from the executed txs
        .with_sidecars(blob_sidecars);

    Ok(BuildOutcome::Better { payload, cached_reads })
}
