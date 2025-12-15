use alloy_consensus::{BlockHeader, Header, Sealable, EMPTY_OMMER_ROOT_HASH};
use alloy_eips::eip7840::BlobParams;
use alloy_primitives::Address;
use botanix_authority_edh::{
    extra_data_header::CHAIN_VERSION, header_ext::HeaderExt, nums_secp256k1_pk,
};
use botanix_chainspec::BotanixChainSpec;
use botanix_evm::error::{ConsensusError, InvalidAggregatedPublicKeyError};
use reth::{
    api::FullNodeTypes,
    builder::{components::ConsensusBuilder, BuilderContext},
    consensus::{
        Consensus, ConsensusError as RethConsensusError, FullConsensus,
        HeaderValidator,
    },
    consensus_common::validation::{
        validate_against_parent_4844, validate_against_parent_hash_number,
        validate_cancun_gas,
    },
};
use reth_chainspec::{EthChainSpec, EthereumHardforks};
use reth_consensus_common::validation::{
    validate_4844_header_standalone, validate_against_parent_eip1559_base_fee,
    validate_against_parent_timestamp, validate_block_pre_execution,
    validate_body_against_header, validate_header_base_fee,
    validate_header_gas,
};
use reth_primitives::{
    GotExpected, Receipt, RecoveredBlock, SealedBlock, SealedHeader,
};
use reth_primitives_traits::constants::{
    GAS_LIMIT_BOUND_DIVISOR, MINIMUM_GAS_LIMIT,
};
use reth_provider::BlockExecutionResult;
use std::{convert::Into, sync::Arc};
use tracing::error;

use crate::{node::BotanixNode, BotanixBlock, BotanixPrimitives};

/// A basic Botanix consensus builder.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct BotanixConsensusBuilder;

impl<Node> ConsensusBuilder<Node> for BotanixConsensusBuilder
where
    Node: FullNodeTypes<Types = BotanixNode>,
{
    type Consensus =
        Arc<dyn FullConsensus<BotanixPrimitives, Error = RethConsensusError>>;

    async fn build_consensus(
        self,
        ctx: &BuilderContext<Node>,
    ) -> eyre::Result<Self::Consensus> {
        Ok(Arc::new(BotanixConsensus::new(ctx.chain_spec())))
    }
}

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

impl BotanixConsensus<BotanixChainSpec> {
    /// Validates PoA header standalone according to the authority consensus
    /// rules.
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

        Self::validate_chain_version(edh.chain_version)?;

        // Past genesis NUMS point should never be used as the aggregated public
        // key
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

impl BotanixConsensus<BotanixChainSpec> {
    // Mainly copy and paste from Reth `fn validate_header()`
    // Note: The Botanix chain ignores withdrawals_root since this has no
    // functionality.
    fn validate_header_inner(
        &self,
        header: &SealedHeader,
    ) -> Result<(), botanix_evm::error::ConsensusError> {
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
        // Not using the Reth `validate_header_extra_data()` since we have
        // custom EDH validation checked in
        // `validate_header_standalone()`.
        validate_header_gas(header)
            .map_err(Into::<botanix_evm::error::ConsensusError>::into)?;
        validate_header_base_fee(header, &self.chain_spec)
            .map_err(Into::<botanix_evm::error::ConsensusError>::into)?;

        // Ensures that EIP-4844 fields are valid once cancun is active.
        if self.chain_spec.is_cancun_active_at_timestamp(header.timestamp()) {
            validate_4844_header_standalone(
                header,
                self.chain_spec
                    .blob_params_at_timestamp(header.timestamp())
                    .unwrap_or_else(BlobParams::cancun),
            )
            .map_err(Into::<botanix_evm::error::ConsensusError>::into)?;
        } else if header.blob_gas_used().is_some() {
            return Err(ConsensusError::BlobGasUsedUnexpected);
        } else if header.excess_blob_gas().is_some() {
            return Err(ConsensusError::ExcessBlobGasUnexpected);
        } else if header.parent_beacon_block_root().is_some() {
            return Err(ConsensusError::ParentBeaconBlockRootUnexpected);
        }

        Ok(())
    }

    /// Copy and paste Reth
    /// Checks the gas limit for consistency between parent and self headers.
    ///
    /// The maximum allowable difference between self and parent gas limits is
    /// determined by the parent's gas limit divided by the
    /// [`GAS_LIMIT_BOUND_DIVISOR`].
    fn validate_against_parent_gas_limit<H: BlockHeader>(
        &self,
        header: &SealedHeader<H>,
        parent: &SealedHeader<H>,
    ) -> Result<(), ConsensusError> {
        // Determine the parent gas limit, considering elasticity multiplier on
        // the London fork.
        let parent_gas_limit =
            if !self.chain_spec.is_london_active_at_block(parent.number())
                && self.chain_spec.is_london_active_at_block(header.number())
            {
                parent.gas_limit()
                    * self
                        .chain_spec
                        .base_fee_params_at_timestamp(header.timestamp())
                        .elasticity_multiplier as u64
            } else {
                parent.gas_limit()
            };

        // Check for an increase in gas limit beyond the allowed threshold.
        if header.gas_limit() > parent_gas_limit {
            if header.gas_limit() - parent_gas_limit
                >= parent_gas_limit / GAS_LIMIT_BOUND_DIVISOR
            {
                return Err(ConsensusError::GasLimitInvalidIncrease {
                    parent_gas_limit,
                    child_gas_limit: header.gas_limit(),
                });
            }
        }
        // Check for a decrease in gas limit beyond the allowed threshold.
        else if parent_gas_limit - header.gas_limit()
            >= parent_gas_limit / GAS_LIMIT_BOUND_DIVISOR
        {
            return Err(ConsensusError::GasLimitInvalidDecrease {
                parent_gas_limit,
                child_gas_limit: header.gas_limit(),
            });
        }
        // Check if the self gas limit is below the minimum required limit.
        else if header.gas_limit() < MINIMUM_GAS_LIMIT {
            return Err(ConsensusError::GasLimitInvalidMinimum {
                child_gas_limit: header.gas_limit(),
            });
        }

        Ok(())
    }
}

impl HeaderValidator for BotanixConsensus<BotanixChainSpec> {
    fn validate_header(
        &self,
        header: &SealedHeader,
    ) -> Result<(), reth_consensus::ConsensusError> {
        self.validate_header_inner(header).map_err(|e| e.into())
    }

    /// Copy and paste from Reth
    fn validate_header_against_parent(
        &self,
        header: &SealedHeader,
        parent: &SealedHeader,
    ) -> Result<(), reth_consensus::ConsensusError> {
        validate_against_parent_hash_number(header.header(), parent)?;

        validate_against_parent_timestamp(header.header(), parent.header())?;

        self.validate_against_parent_gas_limit(header, parent)?;

        validate_against_parent_eip1559_base_fee(
            header.header(),
            parent.header(),
            &self.chain_spec,
        )?;

        // ensure that the blob gas fields for this block
        if let Some(blob_params) =
            self.chain_spec.blob_params_at_timestamp(header.timestamp())
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

use crate::{
    consensus::{MAXIMUM_EXTRA_DATA_SIZE, MAX_EDH_SIZE},
    BotanixBlockBody,
};

impl Consensus<BotanixBlock> for BotanixConsensus<BotanixChainSpec> {
    type Error = RethConsensusError;

    fn validate_body_against_header(
        &self,
        body: &BotanixBlockBody,
        header: &SealedHeader,
    ) -> Result<(), RethConsensusError> {
        validate_body_against_header(body, header.header()).map_err(|e| {
            RethConsensusError::Other(
                format!("body validation failed: {}", e).into(),
            )
        })?;

        Ok(())
    }

    // Copy and paste from Reth `validate_block_pre_execution()` but removing
    // the withdrawals validation
    fn validate_block_pre_execution(
        &self,
        block: &SealedBlock<BotanixBlock>,
    ) -> Result<(), RethConsensusError> {
        // Check ommers hash
        let ommers_hash = block.body().calculate_ommers_root();
        if block.ommers_hash() != ommers_hash {
            return Err(RethConsensusError::BodyOmmersHashDiff(
                GotExpected { got: ommers_hash, expected: block.ommers_hash() }
                    .into(),
            ));
        }

        // Check transaction root
        if let Err(error) = block.ensure_transaction_root_valid() {
            return Err(RethConsensusError::BodyTransactionRootDiff(
                error.into(),
            ));
        }

        if self.chain_spec.is_cancun_active_at_timestamp(block.timestamp()) {
            validate_cancun_gas(block)?;
        }

        Ok(())
    }
}

impl FullConsensus<BotanixPrimitives> for BotanixConsensus<BotanixChainSpec> {
    fn validate_block_post_execution(
        &self,
        _block: &RecoveredBlock<BotanixBlock>,
        _result: &BlockExecutionResult<Receipt>,
    ) -> Result<(), RethConsensusError> {
        // TODO: implement post-execution validation
        Ok(())
    }
}
