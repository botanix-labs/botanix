use crate::node::{engine::BotanixBuiltPayload, engine_api::validator::BotanixExecutionData};
use reth::{
    payload::EthPayloadBuilderAttributes,
    primitives::{NodePrimitives, SealedBlock},
};
use reth_node_ethereum::engine::EthPayloadAttributes;
use reth_payload_primitives::{BuiltPayload, PayloadTypes};

/// A default payload type for [`BotanixPayloadTypes`]
#[derive(Debug, Default, Clone, serde::Deserialize, serde::Serialize)]
#[non_exhaustive]
pub struct BotanixPayloadTypes;

impl PayloadTypes for BotanixPayloadTypes {
    type BuiltPayload = BotanixBuiltPayload;
    type PayloadAttributes = EthPayloadAttributes;
    type PayloadBuilderAttributes = EthPayloadBuilderAttributes;
    type ExecutionData = BotanixExecutionData;

    fn block_to_payload(
        block: SealedBlock<
            <<Self::BuiltPayload as BuiltPayload>::Primitives as NodePrimitives>::Block,
        >,
    ) -> Self::ExecutionData {
        BotanixExecutionData(block.into_block())
    }
}
