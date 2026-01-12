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
use reth_ethereum_consensus::validate_block_post_execution;
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

        // there should always be an aggregate public key for poa
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
    // Currently not being used. For Botanix purposes, this effectively
    // `Compares the gas used in the block header to the actual gas usage after execution.`
    // Since the gas used is taken after block execution and added to the header, this check is not needed.
    fn validate_block_post_execution(
        &self,
        block: &RecoveredBlock<BotanixBlock>,
        result: &BlockExecutionResult<Receipt>,
    ) -> Result<(), RethConsensusError> {
        validate_block_post_execution(
            block,
            &self.chain_spec.inner(),
            &result.receipts,
            &result.requests,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::consensus::MAXIMUM_EXTRA_DATA_SIZE;
    use alloy_consensus::Header;
    use alloy_eips::merge::ALLOWED_FUTURE_BLOCK_TIME_SECONDS;
    use alloy_primitives::{Address, Bytes};
    use botanix_authority_edh::extra_data_header::{
        ExtraDataHeader, CHAIN_VERSION,
    };
    use botanix_authority_edh::header_ext::HeaderExt;
    use botanix_authority_edh::nums_secp256k1_pk;
    use botanix_authority_rsp::{RandomSource, RandomSourceProvider};
    use botanix_chainspec::constants::BOTANIX_TESTNET;
    use botanix_consensus_common::utils::is_inturn;
    use botanix_evm::error::{ConsensusError, InvalidAggregatedPublicKeyError};
    use std::{str::FromStr, sync::Arc};

    use super::*;

    #[allow(dead_code)]
    const EDH_DEFAULT_SIGHASH: &str =
        "0xaaa3492fe3eec8da1ca35aca5930a44b1a5805e813bdd1773678b5041d905276";

    #[allow(dead_code)]
    const SK1: &str =
        "1aabc5cc52b62b570dc69001f1ab49cd1a7056bf6312fe058f094135f2c9b019";
    #[allow(dead_code)]
    const SK2: &str =
        "1bc1f5cc52b62b570dc69001f1ab49cd1a7056bf6312fe058f094135f2c9b019";

    // Tests for validating poa extra data header
    #[test]
    fn should_skip_over_genesis() {
        let consensus = BotanixConsensus::new(Arc::new(
            BOTANIX_TESTNET.as_ref().to_owned(),
        ));
        let header = Header { number: 0, ..Default::default() };
        let authority_signers = vec![];
        // Just use the first key as the dummy agg key
        let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
        let dummy_agg_key = sk1.public_key(secp256k1::SECP256K1);

        let result = consensus.validate_extra_data_header(
            &header,
            &authority_signers,
            Some(&dummy_agg_key),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn fails_when_edh_exceeds_max_size() {
        let consensus = BotanixConsensus::new(Arc::new(
            BOTANIX_TESTNET.as_ref().to_owned(),
        ));
        // In this case we are signing with a non federation different key
        let mut edh = ExtraDataHeader::default();
        let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();

        // Just use the first key as the dummy agg key
        let dummy_agg_key = sk1.public_key(secp256k1::SECP256K1);
        edh.aggregated_public_key = dummy_agg_key;

        let authority_signers = vec![sk1.public_key(secp256k1::SECP256K1)];
        let header = Header {
            number: 1,
            extra_data: Bytes::from([1; MAXIMUM_EXTRA_DATA_SIZE + 1]),
            ..Default::default()
        };

        let result = consensus.validate_extra_data_header(
            &header,
            &authority_signers,
            Some(&dummy_agg_key),
        );
        assert!(result.is_err());
        assert!(matches!(
            result.err().unwrap(),
            ConsensusError::ExtraDataExceedsMax { len } if len == MAXIMUM_EXTRA_DATA_SIZE + 1
        ));
    }

    #[test]
    fn fails_when_edh_has_no_agg_pk() {
        let consensus = BotanixConsensus::new(Arc::new(
            BOTANIX_TESTNET.as_ref().to_owned(),
        ));
        let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
        let authority_signers = vec![sk1.public_key(secp256k1::SECP256K1)];
        let header = Header { number: 1, ..Default::default() };

        let result = consensus.validate_extra_data_header(
            &header,
            &authority_signers,
            None,
        );
        assert!(result.is_err());
        assert!(matches!(
            result.err().unwrap(),
            ConsensusError::InvalidAggregatedPublicKey(
                InvalidAggregatedPublicKeyError::MissingAggregatedPublicKey
            )
        ));
    }

    #[test]
    fn fails_with_invalid_edh() {
        let consensus = BotanixConsensus::new(Arc::new(
            BOTANIX_TESTNET.as_ref().to_owned(),
        ));
        // Just use the first key as the dummy agg key
        let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
        let dummy_agg_key = sk1.public_key(secp256k1::SECP256K1);

        let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
        let authority_signers = vec![sk1.public_key(secp256k1::SECP256K1)];
        let header = Header {
            number: 1,
            extra_data: Bytes::from([0; 64]),
            ..Default::default()
        };

        let result = consensus.validate_extra_data_header(
            &header,
            &authority_signers,
            Some(&dummy_agg_key),
        );
        assert!(result.is_err());
        assert!(matches!(
            result.err().unwrap(),
            ConsensusError::ExtraDataInvalid
        ));
    }

    #[test]
    fn should_not_accept_edh_with_nums_point_past_genesis() {
        let consensus = BotanixConsensus::new(Arc::new(
            BOTANIX_TESTNET.as_ref().to_owned(),
        ));
        // By default edh will use the nums point
        let edh = ExtraDataHeader::default();

        // Just use the first key as the dummy agg key
        let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
        let dummy_agg_key = sk1.public_key(secp256k1::SECP256K1);

        let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
        let authority_signers = vec![sk1.public_key(secp256k1::SECP256K1)];
        let header = Header {
            number: 1,
            extra_data: Bytes::from(edh.serialize()),
            ..Default::default()
        };

        let result = consensus.validate_extra_data_header(
            &header,
            &authority_signers,
            Some(&dummy_agg_key),
        );
        assert!(matches!(
            result.err().unwrap(),
            ConsensusError::InvalidAggregatedPublicKey(
                InvalidAggregatedPublicKeyError::NumsAggregatePublicKeyPastGenesis
            )
        ));
    }

    #[test]
    fn should_not_accept_edh_with_exact_nums_point() {
        let consensus = BotanixConsensus::new(Arc::new(
            BOTANIX_TESTNET.as_ref().to_owned(),
        ));
        // By default edh will use the nums point
        let edh = ExtraDataHeader {
            aggregated_public_key: nums_secp256k1_pk(),
            ..Default::default()
        };
        let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
        let authority_signers = vec![sk1.public_key(secp256k1::SECP256K1)];
        let header = Header {
            number: 1,
            extra_data: Bytes::from(edh.serialize()),
            ..Default::default()
        };

        let result = consensus.validate_extra_data_header(
            &header,
            &authority_signers,
            Some(&nums_secp256k1_pk()),
        );
        assert!(matches!(
            result.err().unwrap(),
            ConsensusError::InvalidAggregatedPublicKey(
                InvalidAggregatedPublicKeyError::NumsAggregatePublicKeyPastGenesis
            )
        ));
    }

    #[test]
    fn should_not_accept_edh_with_invalid_agg_pk() {
        let consensus = BotanixConsensus::new(Arc::new(
            BOTANIX_TESTNET.as_ref().to_owned(),
        ));
        // By default edh will use the nums point
        let mut edh = ExtraDataHeader::default();

        // Just use the first key as the dummy agg key
        let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
        let dummy_agg_key = sk1.public_key(secp256k1::SECP256K1);

        edh.aggregated_public_key = dummy_agg_key;

        let different_key = secp256k1::SecretKey::from_str(SK2).unwrap();
        let different_pk = different_key.public_key(secp256k1::SECP256K1);

        let sk1 = secp256k1::SecretKey::from_str(SK1).unwrap();
        let authority_signers = vec![sk1.public_key(secp256k1::SECP256K1)];
        let header = Header {
            number: 1,
            extra_data: Bytes::from(edh.serialize()),
            ..Default::default()
        };

        let result = consensus.validate_extra_data_header(
            &header,
            &authority_signers,
            Some(&different_pk),
        );
        assert!(matches!(
            result.err().unwrap(),
            ConsensusError::InvalidAggregatedPublicKey(
                InvalidAggregatedPublicKeyError::InvalidAggregatedPublicKey
            )
        ));
    }

    #[test]
    fn unix_timestamp() {
        let timestamp = botanix_consensus_common::utils::unix_timestamp();
        assert!(timestamp > 0);
    }

    #[test]
    fn should_validate_poa_block_beneficiary() {
        // default beneficiary is the burn address
        let consensus = BotanixConsensus::new(Arc::new(
            BOTANIX_TESTNET.as_ref().to_owned(),
        ));
        let header = Header::default();
        let result = consensus.validate_block_beneficiary(&header);
        assert!(result.is_ok());
    }

    #[test]
    fn should_fail_validate_poa_block_beneficiary() {
        let consensus = BotanixConsensus::new(Arc::new(
            BOTANIX_TESTNET.as_ref().to_owned(),
        ));
        let header = Header {
            beneficiary: Address::from_str(
                "0x4e0f6e05C8ca4b3dc2B7b7Ad6249B149b1980394",
            )
            .unwrap(),
            ..Default::default()
        };
        let result = consensus.validate_block_beneficiary(&header);
        assert!(result.is_err());
    }

    #[test]
    fn is_inturn_true() {
        let authorities_len = 1;
        let signer_index = 0;
        let random_source = RandomSourceProvider::new().random_source();
        assert!(is_inturn(
            authorities_len,
            signer_index,
            ALLOWED_FUTURE_BLOCK_TIME_SECONDS,
            random_source
        ));
    }

    #[test]
    fn is_inturn_false() {
        let authorities_len = 1;
        let signer_index = 1;
        let random_source = RandomSourceProvider::new().random_source();

        assert!(!is_inturn(
            authorities_len,
            signer_index,
            ALLOWED_FUTURE_BLOCK_TIME_SECONDS,
            random_source
        ));
    }

    #[test]
    fn should_get_block_fee_recipient_address_from_header() {
        let mut header = Header::default();
        let edh = ExtraDataHeader::default();
        header.add_extra_data_header(&edh);
        let block_fee_recipient_address =
            header.block_fee_recipient_address().unwrap();
        assert_eq!(block_fee_recipient_address, Address::ZERO);

        let mut header2 = Header::default();
        let edh2 = ExtraDataHeader {
            block_fee_recipient_address: Address::from_str(
                "0x4e0f6e05C8ca4b3dc2B7b7Ad6249B149b1980394",
            )
            .unwrap(),
            ..Default::default()
        };
        header2.add_extra_data_header(&edh2);
        let block_producer_address2 =
            header2.block_fee_recipient_address().unwrap();
        assert_eq!(block_producer_address2, edh2.block_fee_recipient_address);
    }

    #[test]
    fn should_validate_chain_version() {
        let edh_chain_version = CHAIN_VERSION;
        let result =
            BotanixConsensus::validate_chain_version(edh_chain_version);
        assert!(result.is_ok());

        let edh_chain_version = CHAIN_VERSION + 1;
        let result =
            BotanixConsensus::validate_chain_version(edh_chain_version);
        assert!(result.is_err());
    }
}
