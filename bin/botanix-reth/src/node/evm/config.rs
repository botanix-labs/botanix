/// Ethereum-related EVM configuration.
use super::{executor::BotanixBlockExecutor, factory::BotanixEvmFactory};
use crate::{
    evm::transaction::BotanixTxEnv,
    node::engine_api::validator::BotanixExecutionData, BotanixPrimitives,
};
use alloy_consensus::{
    transaction::SignerRecoverable, BlockHeader, Header, TxReceipt,
};
use alloy_eips::eip7840::BlobParams;
use alloy_primitives::{Log, U256};
use botanix_chainspec::{BotanixChainSpec, BotanixHardfork, BotanixHardforks};
use reth_chainspec::{EthChainSpec, EthereumHardforks, Hardforks};
use reth_ethereum_forks::EthereumHardfork;
use reth_evm::{
    block::{BlockExecutorFactory, BlockExecutorFor},
    eth::{receipt_builder::ReceiptBuilder, EthBlockExecutionCtx},
    ConfigureEngineEvm, ConfigureEvm, EvmEnv, EvmFactory, ExecutableTxIterator,
    ExecutionCtxFor, FromRecoveredTx, FromTxWithEncoded, IntoTxEnv,
    NextBlockEnvAttributes,
};
use reth_evm_ethereum::{
    revm_spec_by_timestamp_and_block_number as reth_revm_spec_by_timestamp_and_block_number,
    EthBlockAssembler, RethReceiptBuilder,
};
use reth_primitives::{
    BlockTy, HeaderTy, SealedBlock, SealedHeader, TransactionSigned,
};
use reth_revm::State;
use revm::{
    context::{BlockEnv, CfgEnv},
    context_interface::block::BlobExcessGasAndPrice,
    primitives::hardfork::SpecId,
    Inspector,
};
use std::{borrow::Cow, convert::Infallible, sync::Arc};

/// Ethereum-related EVM configuration.
#[derive(Debug, Clone)]
pub struct BotanixEvmConfig {
    /// Inner [`BotanixBlockExecutorFactory`].
    pub executor_factory: BotanixBlockExecutorFactory<
        RethReceiptBuilder,
        Arc<BotanixChainSpec>,
        BotanixEvmFactory,
    >,
    /// Ethereum block assembler.
    pub block_assembler: EthBlockAssembler<BotanixChainSpec>,
}

impl BotanixEvmConfig {
    /// Creates a new Ethereum EVM configuration with the given chain spec.
    pub fn new(chain_spec: Arc<BotanixChainSpec>) -> Self {
        Self::botanix(chain_spec)
    }

    /// Creates a new Ethereum EVM configuration.
    pub fn botanix(chain_spec: Arc<BotanixChainSpec>) -> Self {
        Self::new_with_evm_factory(chain_spec, BotanixEvmFactory::default())
    }
}

impl BotanixEvmConfig {
    /// Creates a new Ethereum EVM configuration with the given chain spec and
    /// EVM factory.
    pub fn new_with_evm_factory(
        chain_spec: Arc<BotanixChainSpec>,
        evm_factory: BotanixEvmFactory,
    ) -> Self {
        Self {
            block_assembler: EthBlockAssembler::new(chain_spec.clone()),
            executor_factory: BotanixBlockExecutorFactory::new(
                RethReceiptBuilder::default(),
                chain_spec,
                evm_factory,
            ),
        }
    }

    /// Returns the chain spec associated with this configuration.
    pub const fn chain_spec(&self) -> &Arc<BotanixChainSpec> {
        self.executor_factory.spec()
    }
}

/// Ethereum block executor factory.
#[derive(Debug, Clone, Default, Copy)]
pub struct BotanixBlockExecutorFactory<
    R = RethReceiptBuilder,
    Spec = Arc<BotanixChainSpec>,
    EvmFactory = BotanixEvmFactory,
> {
    /// Receipt builder.
    receipt_builder: R,
    /// Chain specification.
    spec: Spec,
    /// EVM factory.
    evm_factory: EvmFactory,
}

impl<R, Spec, EvmFactory> BotanixBlockExecutorFactory<R, Spec, EvmFactory> {
    /// Creates a new [`BotanixBlockExecutorFactory`] with the given spec,
    /// [`EvmFactory`], and [`ReceiptBuilder`].
    pub const fn new(
        receipt_builder: R,
        spec: Spec,
        evm_factory: EvmFactory,
    ) -> Self {
        Self { receipt_builder, spec, evm_factory }
    }

    /// Exposes the receipt builder.
    pub const fn receipt_builder(&self) -> &R {
        &self.receipt_builder
    }

    /// Exposes the chain specification.
    pub const fn spec(&self) -> &Spec {
        &self.spec
    }
}

impl<R, Spec, EvmF> BlockExecutorFactory
    for BotanixBlockExecutorFactory<R, Spec, EvmF>
where
    R: ReceiptBuilder<
        Transaction = TransactionSigned,
        Receipt: TxReceipt<Log = Log>,
    >,
    Spec:
        EthereumHardforks + BotanixHardforks + EthChainSpec + Hardforks + Clone,
    EvmF: EvmFactory<
        Tx: FromRecoveredTx<TransactionSigned>
                + FromTxWithEncoded<TransactionSigned>,
    >,
    R::Transaction: From<TransactionSigned> + Clone,
    Self: 'static,
    BotanixTxEnv: IntoTxEnv<<EvmF as EvmFactory>::Tx>,
{
    type EvmFactory = EvmF;
    type ExecutionCtx<'a> = EthBlockExecutionCtx<'a>;
    type Transaction = TransactionSigned;
    type Receipt = R::Receipt;

    fn evm_factory(&self) -> &Self::EvmFactory {
        &self.evm_factory
    }

    fn create_executor<'a, DB, I>(
        &'a self,
        evm: <Self::EvmFactory as EvmFactory>::Evm<&'a mut State<DB>, I>,
        ctx: Self::ExecutionCtx<'a>,
    ) -> impl BlockExecutorFor<'a, Self, DB, I>
    where
        DB: alloy_evm::Database + 'a,
        I: Inspector<
                <Self::EvmFactory as EvmFactory>::Context<&'a mut State<DB>>,
            > + 'a,
    {
        BotanixBlockExecutor::new(
            evm,
            ctx,
            self.spec().clone(),
            self.receipt_builder(),
        )
    }
}

const EIP1559_INITIAL_BASE_FEE: u64 = 0;

impl ConfigureEvm for BotanixEvmConfig
where
    Self: Send + Sync + Unpin + Clone + 'static,
{
    type Primitives = BotanixPrimitives;
    type Error = Infallible;
    type NextBlockEnvCtx = NextBlockEnvAttributes;
    type BlockExecutorFactory = BotanixBlockExecutorFactory;
    type BlockAssembler = Self;

    fn block_executor_factory(&self) -> &Self::BlockExecutorFactory {
        &self.executor_factory
    }

    fn block_assembler(&self) -> &Self::BlockAssembler {
        self
    }

    fn evm_env(&self, header: &Header) -> EvmEnv<BotanixHardfork> {
        let blob_params =
            self.chain_spec().blob_params_at_timestamp(header.timestamp);
        let spec = revm_spec_by_timestamp_and_block_number(
            self.chain_spec().clone(),
            header.timestamp(),
            header.number(),
        );

        // configure evm env based on parent block
        let mut cfg_env = CfgEnv::new()
            .with_chain_id(self.chain_spec().chain().id())
            .with_spec(spec);

        if let Some(blob_params) = &blob_params {
            cfg_env.set_max_blobs_per_tx(blob_params.max_blobs_per_tx);
        }

        // derive the EIP-4844 blob fees from the header's `excess_blob_gas` and
        // the current blobparams
        let blob_excess_gas_and_price = header
            .excess_blob_gas
            .zip(blob_params)
            .map(|(excess_blob_gas, params)| {
                let blob_gasprice = params.calc_blob_fee(excess_blob_gas);
                BlobExcessGasAndPrice { excess_blob_gas, blob_gasprice }
            });

        let eth_spec = SpecId::from(spec);

        let block_env = BlockEnv {
            number: U256::from(header.number()),
            beneficiary: header.beneficiary(),
            timestamp: U256::from(header.timestamp()),
            difficulty: if eth_spec >= SpecId::MERGE {
                U256::ZERO
            } else {
                header.difficulty()
            },
            // Botanix does not replace the DIFFICULTY output with prevrandao so
            // here we are setting this to the difficulty values to
            // ensure correct opcode outputs
            prevrandao: if eth_spec >= SpecId::MERGE {
                Some(header.difficulty().into())
            } else {
                None
            },
            gas_limit: header.gas_limit(),
            basefee: header.base_fee_per_gas().unwrap_or_default(),
            blob_excess_gas_and_price,
        };

        EvmEnv { cfg_env, block_env }
    }

    fn next_evm_env(
        &self,
        parent: &Header,
        attributes: &Self::NextBlockEnvCtx,
    ) -> Result<EvmEnv<BotanixHardfork>, Self::Error> {
        // ensure we're not missing any timestamp based hardforks
        let spec_id = revm_spec_by_timestamp_and_block_number(
            self.chain_spec().clone(),
            attributes.timestamp,
            parent.number() + 1,
        );

        // configure evm env based on parent block
        let cfg_env = CfgEnv::new()
            .with_chain_id(self.chain_spec().chain().id())
            .with_spec(spec_id);

        let blob_params =
            self.chain_spec().blob_params_at_timestamp(attributes.timestamp);

        // if the parent block did not have excess blob gas (i.e. it was
        // pre-cancun), but it is cancun now, we need to set the excess
        // blob gas to the default value(0)
        let blob_excess_gas_and_price = parent
            .maybe_next_block_excess_blob_gas(blob_params)
            .or_else(|| {
                (SpecId::from(spec_id).is_enabled_in(SpecId::CANCUN))
                    .then_some(0)
            })
            .map(|excess_blob_gas| {
                let blob_gasprice = blob_params
                    .unwrap_or_else(BlobParams::cancun)
                    .calc_blob_fee(excess_blob_gas);
                BlobExcessGasAndPrice { excess_blob_gas, blob_gasprice }
            });

        let mut basefee = parent.next_block_base_fee(
            self.chain_spec()
                .base_fee_params_at_timestamp(attributes.timestamp),
        );

        let mut gas_limit = U256::from(parent.gas_limit);

        // If we are on the London fork boundary, we need to multiply the
        // parent's gas limit by the elasticity multiplier to get the
        // new gas limit.
        if self
            .chain_spec()
            .inner
            .fork(EthereumHardfork::London)
            .transitions_at_block(parent.number + 1)
        {
            let elasticity_multiplier = self
                .chain_spec()
                .base_fee_params_at_timestamp(attributes.timestamp)
                .elasticity_multiplier;

            // multiply the gas limit by the elasticity multiplier
            gas_limit *= U256::from(elasticity_multiplier);

            // set the base fee to the initial base fee from the EIP-1559 spec
            basefee = Some(EIP1559_INITIAL_BASE_FEE)
        }

        let block_env = BlockEnv {
            number: U256::from(parent.number() + 1),
            beneficiary: attributes.suggested_fee_recipient,
            timestamp: U256::from(attributes.timestamp),
            difficulty: U256::ZERO,
            prevrandao: Some(attributes.prev_randao),
            gas_limit: attributes.gas_limit,
            // calculate basefee based on parent block's gas usage
            basefee: basefee.unwrap_or_default(),
            // calculate excess gas based on parent block's blob gas usage
            blob_excess_gas_and_price,
        };

        Ok(EvmEnv { cfg_env, block_env })
    }

    fn context_for_block<'a>(
        &self,
        block: &'a SealedBlock<BlockTy<Self::Primitives>>,
    ) -> ExecutionCtxFor<'a, Self> {
        EthBlockExecutionCtx {
            parent_hash: block.header().parent_hash,
            parent_beacon_block_root: block.header().parent_beacon_block_root,
            ommers: &block.body().ommers,
            withdrawals: block.body().withdrawals.as_ref().map(Cow::Borrowed),
        }
    }

    fn context_for_next_block(
        &self,
        parent: &SealedHeader<HeaderTy<Self::Primitives>>,
        attributes: Self::NextBlockEnvCtx,
    ) -> ExecutionCtxFor<'_, Self> {
        EthBlockExecutionCtx {
            parent_hash: parent.hash(),
            parent_beacon_block_root: attributes.parent_beacon_block_root,
            ommers: &[],
            withdrawals: attributes.withdrawals.map(Cow::Owned),
        }
    }
}

impl ConfigureEngineEvm<BotanixExecutionData> for BotanixEvmConfig
where
    Self: Send + Sync + Unpin + Clone + 'static,
{
    fn evm_env_for_payload(
        &self,
        payload: &BotanixExecutionData,
    ) -> EvmEnv<BotanixHardfork> {
        self.evm_env(&payload.0.header)
    }

    fn context_for_payload<'a>(
        &self,
        payload: &'a BotanixExecutionData,
    ) -> EthBlockExecutionCtx<'a> {
        let block = &payload.0;
        EthBlockExecutionCtx {
            parent_hash: block.header.parent_hash(),
            parent_beacon_block_root: block.header.parent_beacon_block_root,
            ommers: &block.body.inner.ommers,
            withdrawals: block
                .body
                .inner
                .withdrawals
                .as_ref()
                .map(Cow::Borrowed),
        }
    }

    fn tx_iterator_for_payload(
        &self,
        payload: &BotanixExecutionData,
    ) -> impl ExecutableTxIterator<Self> {
        payload
            .0
            .body
            .inner
            .transactions
            .clone()
            .into_iter()
            .map(|tx| tx.try_into_recovered())
    }
}

/// Map the latest active hardfork at the given timestamp or block number to a
/// [`BotanixHardfork`].
/// Currently just a wrapper for the external reth function.
pub fn revm_spec_by_timestamp_and_block_number(
    chain_spec: impl BotanixHardforks + EthChainSpec + Hardforks,
    timestamp: u64,
    block_number: u64,
) -> BotanixHardfork {
    let spec_id = reth_revm_spec_by_timestamp_and_block_number(
        &chain_spec,
        timestamp,
        block_number,
    );
    spec_id.into()
}
