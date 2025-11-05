use alloy_consensus::Header;
use alloy_primitives::B256;
use botanix_chainspec::{BotanixChainSpec, BotanixHardforks};
use reth::{
    api::FullNodeTypes,
    builder::{components::ConsensusBuilder, BuilderContext},
    consensus::{Consensus, ConsensusError, FullConsensus, HeaderValidator},
    consensus_common::validation::{
        validate_against_parent_4844, validate_against_parent_hash_number,
    },
};
use reth_chainspec::EthChainSpec;
use reth_consensus_common::validation::validate_block_pre_execution;
use reth_primitives::{Receipt, RecoveredBlock, SealedBlock, SealedHeader};
use reth_provider::BlockExecutionResult;
use std::sync::Arc;

use crate::BotanixBlock;

/// A basic Botanix consensus builder.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct BotanixConsensusBuilder;

// impl<Node> ConsensusBuilder<Node> for BotanixConsensusBuilder
// where
//     Node: FullNodeTypes<Types = BotanixNode>,
// {
//     type Consensus =
//         Arc<dyn FullConsensus<BotanixPrimitives, Error = ConsensusError>>;

//     async fn build_consensus(
//         self,
//         ctx: &BuilderContext<Node>,
//     ) -> eyre::Result<Self::Consensus> {
//         Ok(Arc::new(BotanixConsensus::new(ctx.chain_spec())))
//     }
// }

/// Botanix consensus implementation.
///
/// Provides basic checks as outlined in the execution specs.
#[derive(Debug, Clone)]
pub struct BotanixConsensus<BotanixChainSpec> {
    chain_spec: Arc<BotanixChainSpec>,
}

impl BotanixConsensus<BotanixChainSpec> {
    /// Create a new instance of [`BotanixConsensus`]
    pub fn new(chain_spec: Arc<BotanixChainSpec>) -> Self {
        Self { chain_spec }
    }
}


// impl FullConsensus<BlockWithSenders> for BotanixConsensus<BotanixChainSpec> {
//     fn validate_block_post_execution(
//         &self,
//         block: &BlockWithSenders,
//         input: BlockExecutionResult<_>,
//     ) -> Result<(), ConsensusError> {
//         validate_block_post_execution(
//             block,
//             &self.chain_spec.inner(),
//             input.receipts,
//             input.requests,
//         )
//     }
// }

impl<ChainSpec: EthChainSpec + BotanixHardforks> HeaderValidator
    for BotanixConsensus<ChainSpec>
{
    fn validate_header(
        &self,
        _header: &SealedHeader,
    ) -> Result<(), ConsensusError> {
        // TODO: doesn't work because of extradata check
        // self.inner.validate_header(header)

        Ok(())
    }

    fn validate_header_against_parent(
        &self,
        header: &SealedHeader,
        parent: &SealedHeader,
    ) -> Result<(), ConsensusError> {
        validate_against_parent_hash_number(header.header(), parent)?;

        let header_ts = calculate_millisecond_timestamp(header.header());
        let parent_ts = calculate_millisecond_timestamp(parent.header());
        if header_ts <= parent_ts {
            return Err(ConsensusError::TimestampIsInPast {
                parent_timestamp: parent_ts,
                timestamp: header_ts,
            });
        }

        // ensure that the blob gas fields for this block
        if let Some(blob_params) =
            self.chain_spec.blob_params_at_timestamp(header.timestamp)
        {
            validate_against_parent_4844(
                header.header(),
                parent.header(),
                blob_params,
            )?;
        }

        Ok(())
    }
}

use crate::BotanixBlockBody;

impl Consensus<BotanixBlock> for BotanixConsensus<BotanixChainSpec> {
    type Error = ConsensusError;

    fn validate_body_against_header(
        &self,
        body: &BotanixBlockBody,
        header: &SealedHeader,
    ) -> Result<(), ConsensusError> {
        // Delegate to standard Ethereum block body validation using the inner body
        Consensus::<reth_primitives::Block>::validate_body_against_header(
            self,
            &body.inner,
            header,
        )
    }

    fn validate_block_pre_execution(
        &self,
        block: &SealedBlock<BotanixBlock>,
    ) -> Result<(), ConsensusError> {
        validate_block_pre_execution(block, &self.chain_spec.inner())
    }
}

// impl<ChainSpec: EthChainSpec<Header = Header> + BotanixHardforks>
//     FullConsensus<BotanixPrimitives> for BotanixConsensus<ChainSpec>
// {
//     fn validate_block_post_execution(
//         &self,
//         block: &RecoveredBlock<BotanixBlock>,
//         result: &BlockExecutionResult<Receipt>,
//     ) -> Result<(), ConsensusError> {
//         FullConsensus::<BotanixPrimitives>::validate_block_post_execution(
//             &self.inner,
//             block,
//             result,
//         )
//     }
// }

/// Calculate the millisecond timestamp of a block header.
/// Refer to https://github.com/bnb-chain/BEPs/blob/master/BEPs/BEP-520.md.
pub fn calculate_millisecond_timestamp<H: alloy_consensus::BlockHeader>(
    header: &H,
) -> u64 {
    let seconds = header.timestamp();
    let mix_digest = header.mix_hash().unwrap_or(B256::ZERO);

    let milliseconds = if mix_digest != B256::ZERO {
        let bytes = mix_digest.as_slice();
        // Convert last 8 bytes to u64 (big-endian), equivalent to Go's
        // uint256.SetBytes32().Uint64()
        let mut result = 0u64;
        for &byte in bytes.iter().skip(24).take(8) {
            result = (result << 8) | u64::from(byte);
        }
        result
    } else {
        0
    };

    seconds * 1000 + milliseconds
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_consensus::Header;
    use alloy_primitives::B256;

    #[test]
    fn test_calculate_millisecond_timestamp_without_mix_hash() {
        // Create a header with current timestamp and zero mix_hash
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let header = Header {
            timestamp,
            mix_hash: B256::ZERO,
            ..Default::default()
        };

        let result = calculate_millisecond_timestamp(&header);
        assert_eq!(result, timestamp * 1000);
    }

    #[test]
    fn test_calculate_millisecond_timestamp_with_milliseconds() {
        // Create a header with current timestamp and mix_hash containing milliseconds
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let milliseconds = 750u64;
        let mut mix_hash_bytes = [0u8; 32];
        mix_hash_bytes[24..32].copy_from_slice(&milliseconds.to_be_bytes());
        let mix_hash = B256::new(mix_hash_bytes);

        let header = Header {
            timestamp,
            mix_hash,
            ..Default::default()
        };

        let result = calculate_millisecond_timestamp(&header);
        assert_eq!(result, timestamp * 1000 + milliseconds);
    }
}
