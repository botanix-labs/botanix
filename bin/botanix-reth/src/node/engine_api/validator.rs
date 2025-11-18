use crate::{BotanixBlock, BotanixPrimitives};
use alloy_consensus::BlockHeader;
use alloy_eips::eip4895::Withdrawal;
use alloy_primitives::B256;
use alloy_rpc_types_engine::PayloadError;
use botanix_chainspec::{BotanixChainSpec, BotanixHardforks};
use reth::{
    api::{FullNodeComponents, NodeTypes},
    builder::{
        rpc::{BasicEngineValidatorBuilder, PayloadValidatorBuilder},
        AddOnsContext,
    },
    consensus::ConsensusError,
};
use reth_engine_primitives::{ExecutionPayload, PayloadValidator};
use reth_payload_primitives::NewPayloadError;
use reth_primitives::{RecoveredBlock, SealedBlock};
use reth_primitives_traits::Block as _;
use reth_trie_common::HashedPostState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::payload::BotanixPayloadTypes;

#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct BotanixPayloadValidatorBuilder;

impl<Node, Types> PayloadValidatorBuilder<Node>
    for BotanixPayloadValidatorBuilder
where
    Types: NodeTypes<
        ChainSpec = BotanixChainSpec,
        Payload = BotanixPayloadTypes,
        Primitives = BotanixPrimitives,
    >,
    Node: FullNodeComponents<Types = Types>,
{
    type Validator = BotanixEngineValidator;

    async fn build(
        self,
        ctx: &AddOnsContext<'_, Node>,
    ) -> eyre::Result<Self::Validator> {
        Ok(BotanixEngineValidator::new(Arc::new(
            ctx.config.chain.clone().as_ref().clone(),
        )))
    }
}

/// Botanix engine validator builder that wraps the payload validator
pub type BotanixEngineValidatorBuilder =
    BasicEngineValidatorBuilder<BotanixPayloadValidatorBuilder>;

/// Validator for Optimism engine API.
#[derive(Debug, Clone)]
pub struct BotanixEngineValidator {
    inner: BotanixExecutionPayloadValidator<BotanixChainSpec>,
}

impl BotanixEngineValidator {
    /// Instantiates a new validator.
    pub fn new(chain_spec: Arc<BotanixChainSpec>) -> Self {
        Self { inner: BotanixExecutionPayloadValidator { inner: chain_spec } }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotanixExecutionData(pub BotanixBlock);

impl ExecutionPayload for BotanixExecutionData {
    fn parent_hash(&self) -> B256 {
        self.0.header.parent_hash()
    }

    fn block_hash(&self) -> B256 {
        self.0.header.hash_slow()
    }

    fn block_number(&self) -> u64 {
        self.0.header.number()
    }

    fn withdrawals(&self) -> Option<&Vec<Withdrawal>> {
        None
    }

    fn parent_beacon_block_root(&self) -> Option<B256> {
        None
    }

    fn timestamp(&self) -> u64 {
        self.0.header.timestamp()
    }

    fn gas_used(&self) -> u64 {
        self.0.header.gas_used()
    }
}

impl PayloadValidator<BotanixPayloadTypes> for BotanixEngineValidator {
    type Block = BotanixBlock;

    fn ensure_well_formed_payload(
        &self,
        payload: BotanixExecutionData,
    ) -> Result<RecoveredBlock<Self::Block>, NewPayloadError> {
        let sealed_block = self
            .inner
            .ensure_well_formed_payload(payload)
            .map_err(NewPayloadError::other)?;
        sealed_block.try_recover().map_err(|e| NewPayloadError::Other(e.into()))
    }

    fn validate_block_post_execution_with_hashed_state(
        &self,
        _state_updates: &HashedPostState,
        _block: &RecoveredBlock<Self::Block>,
    ) -> Result<(), ConsensusError> {
        Ok(())
    }
}

/// Execution payload validator.
#[derive(Clone, Debug)]
pub struct BotanixExecutionPayloadValidator<ChainSpec> {
    /// Chain spec to validate against.
    #[allow(unused)]
    inner: Arc<ChainSpec>,
}

impl<ChainSpec> BotanixExecutionPayloadValidator<ChainSpec>
where
    ChainSpec: BotanixHardforks,
{
    pub fn ensure_well_formed_payload(
        &self,
        payload: BotanixExecutionData,
    ) -> Result<SealedBlock<BotanixBlock>, PayloadError> {
        let block = payload.0;

        let expected_hash = block.header.hash_slow();

        // First parse the block
        let sealed_block = block.seal_slow();

        // Ensure the hash included in the payload matches the block hash
        if expected_hash != sealed_block.hash() {
            return Err(PayloadError::BlockHash {
                execution: sealed_block.hash(),
                consensus: expected_hash,
            })?;
        }

        Ok(sealed_block)
    }
}
