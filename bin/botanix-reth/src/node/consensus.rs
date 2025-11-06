use alloy_consensus::Sealable;
use alloy_consensus::{BlockHeader, Header, EMPTY_OMMER_ROOT_HASH};
use alloy_eips::eip7840::BlobParams;
use alloy_primitives::{Address, B256};
use botanix_authority_edh::extra_data_header::CHAIN_VERSION;
use botanix_authority_edh::nums_secp256k1_pk;
use botanix_authority_edh::{
    extra_data_header::ExtraDataHeader, header_ext::HeaderExt,
};
use botanix_chainspec::{BotanixChainSpec, BotanixHardforks};
use botanix_consensus_common::utils::validate_chain_version;
use botanix_evm::error::{ConsensusError, InvalidAggregatedPublicKeyError};
use reth::{
    api::FullNodeTypes,
    builder::{components::ConsensusBuilder, BuilderContext},
    consensus::{Consensus, FullConsensus, HeaderValidator},
    consensus_common::validation::{
        validate_against_parent_4844, validate_against_parent_hash_number,
    },
};
use reth_chainspec::{EthChainSpec, EthereumHardforks};
use reth_consensus_common::validation::{
    validate_4844_header_standalone, validate_block_pre_execution,
    validate_header_base_fee, validate_header_extra_data, validate_header_gas,
};
use reth_primitives::{Receipt, RecoveredBlock, SealedBlock, SealedHeader};
use reth_provider::BlockExecutionResult;
use std::sync::Arc;
use tracing::error;

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

impl BotanixConsensus<BotanixChainSpec> {
    /// Validates PoA header standalone according to the authority consensus rules.
    pub fn validate_header_standalone(
        &self,
        header: &Header,
        genesis_authorities: &[secp256k1::PublicKey],
        aggregate_public_key: Option<&secp256k1::PublicKey>,
    ) -> Result<(), ConsensusError> {
        if aggregate_public_key.is_none() {
            return Err(ConsensusError::InvalidAggregatedPublicKey(
                InvalidAggregatedPublicKeyError::MissingAggregatedPublicKey,
            ));
        }

        // run the reth header validation rule
        let _sealed_header = header.clone().seal_slow();

        // Validate EDH serialization and signature on block
        self.validate_extra_data_header(
            header,
            genesis_authorities,
            aggregate_public_key,
        )?;

        // Validate fee benificiary
        self.validate_block_beneficiary(header)?;

        Ok(())
    }

    /// Validate poa block beneficiary
    fn validate_block_beneficiary(
        &self,
        header: &Header,
    ) -> Result<(), ConsensusError> {
        if header.beneficiary != Address::ZERO {
            return Err(ConsensusError::BlockBeneficiaryIsNotBurnAddress);
        }

        Ok(())
    }

    /// Validate poa extra data header
    fn validate_extra_data_header(
        &self,
        header: &Header,
        _genesis_authorities: &[secp256k1::PublicKey],
        aggregate_public_key: Option<&secp256k1::PublicKey>,
    ) -> Result<(), ConsensusError> {
        // Skip over genesis
        if header.number == 0 {
            return Ok(());
        }

        // there should alawys be an aggregate public key for poa
        if aggregate_public_key.is_none() {
            return Err(ConsensusError::InvalidAggregatedPublicKey(
                InvalidAggregatedPublicKeyError::MissingAggregatedPublicKey,
            ));
        }

        let extradata_len = header.extra_data.as_ref().len();
        if extradata_len > MAXIMUM_EXTRA_DATA_SIZE {
            return Err(ConsensusError::ExtraDataExceedsMax {
                len: extradata_len,
            });
        }

        // Attempt to deserialize the extra data header
        let edh = header.deserialize_extra_data_header().map_err(|e| {
            error!("Failed to deserialize extra data header: {:?}", e);
            ConsensusError::ExtraDataInvalid
        })?;

        // Check total size of the extra data header
        if edh.edh_size() > MAX_EDH_SIZE {
            return Err(ConsensusError::ExtraDataHeaderExceedsMax {
                len: edh.edh_size(),
            });
        }

        validate_chain_version(edh.chain_version)?;

        // Past genesis NUMS point should never be used as the aggregated public key
        if edh.aggregated_public_key == nums_secp256k1_pk() {
            return Err(ConsensusError::InvalidAggregatedPublicKey(
                InvalidAggregatedPublicKeyError::NumsAggregatePublicKeyPastGenesis,
            ));
        }

        if edh.aggregated_public_key != *aggregate_public_key.unwrap() {
            return Err(ConsensusError::InvalidAggregatedPublicKey(
                InvalidAggregatedPublicKeyError::InvalidAggregatedPublicKey,
            ));
        }

        Ok(())
    }

    /// Check the extra data header field has the current chain version
    pub const fn validate_chain_version(
        edh_chain_version: u32,
    ) -> Result<(), ConsensusError> {
        if edh_chain_version != CHAIN_VERSION {
            return Err(ConsensusError::InvalidChainVersion);
        }

        Ok(())
    }
}

impl HeaderValidator for BotanixConsensus<BotanixChainSpec> {
    // Copied from Reth `crates/ethereum/consensus/src/lib.rs`
    fn validate_header(
        &self,
        header: &SealedHeader,
    ) -> Result<(), ConsensusError> {
        let header = header.header();
        let is_post_merge =
            self.chain_spec.is_paris_active_at_block(header.number());

        if is_post_merge {
            if !header.difficulty().is_zero() {
                return Err(ConsensusError::TheMergeDifficultyIsNotZero);
            }

            if !header.nonce().is_some_and(|nonce| nonce.is_zero()) {
                return Err(ConsensusError::TheMergeNonceIsNotZero);
            }

            if header.ommers_hash() != EMPTY_OMMER_ROOT_HASH {
                return Err(ConsensusError::TheMergeOmmerRootIsNotEmpty);
            }
        } else {
            {
                let present_timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                if header.timestamp()
                    > present_timestamp
                        + alloy_eips::merge::ALLOWED_FUTURE_BLOCK_TIME_SECONDS
                {
                    return Err(ConsensusError::TimestampIsInFuture {
                        timestamp: header.timestamp(),
                        present_timestamp,
                    });
                }
            }
        }
        // Not using the Reth `validate_header_extra_data()` since we have custom EDH validation
        // checked in `validate_header_standalone()`.
        validate_header_gas(header)?;
        validate_header_base_fee(header, &self.chain_spec)?;

        // EIP-4895: Beacon chain push withdrawals as operations
        if self
            .chain_spec
            .is_shanghai_active_at_timestamp(header.timestamp())
            && header.withdrawals_root().is_none()
        {
            return Err(ConsensusError::WithdrawalsRootMissing);
        } else if !self
            .chain_spec
            .is_shanghai_active_at_timestamp(header.timestamp())
            && header.withdrawals_root().is_some()
        {
            return Err(ConsensusError::WithdrawalsRootUnexpected);
        }

        // Ensures that EIP-4844 fields are valid once cancun is active.
        if self
            .chain_spec
            .is_cancun_active_at_timestamp(header.timestamp())
        {
            validate_4844_header_standalone(
                header,
                self.chain_spec
                    .blob_params_at_timestamp(header.timestamp())
                    .unwrap_or_else(BlobParams::cancun),
            )?;
        } else if header.blob_gas_used().is_some() {
            return Err(ConsensusError::BlobGasUsedUnexpected);
        } else if header.excess_blob_gas().is_some() {
            return Err(ConsensusError::ExcessBlobGasUnexpected);
        } else if header.parent_beacon_block_root().is_some() {
            return Err(ConsensusError::ParentBeaconBlockRootUnexpected);
        }

        if self
            .chain_spec
            .is_prague_active_at_timestamp(header.timestamp())
        {
            if header.requests_hash().is_none() {
                return Err(ConsensusError::RequestsHashMissing);
            }
        } else if header.requests_hash().is_some() {
            return Err(ConsensusError::RequestsHashUnexpected);
        }

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

use crate::consensus::{MAXIMUM_EXTRA_DATA_SIZE, MAX_EDH_SIZE};
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
