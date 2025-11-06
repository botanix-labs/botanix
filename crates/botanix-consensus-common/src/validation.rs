//! Collection of methods for block validation.

use alloy_consensus::BlockHeader;
use alloy_eips::eip4844::{
    calc_excess_blob_gas, DATA_GAS_PER_BLOB,
    MAX_DATA_GAS_PER_BLOCK_DENCUN as MAX_DATA_GAS_PER_BLOCK,
};
use reth_chainspec::{ChainSpec, EthereumHardforks};
use reth_consensus::ConsensusError;
use reth_primitives::{
    EthereumHardfork, GotExpected, Header, SealedBlock, SealedHeader,
};

/// Gas used needs to be less than gas limit. Gas used is going to be checked after execution.
#[inline]
pub const fn validate_header_gas(
    header: &Header,
) -> Result<(), ConsensusError> {
    if header.gas_used > header.gas_limit {
        return Err(ConsensusError::HeaderGasUsedExceedsGasLimit {
            gas_used: header.gas_used,
            gas_limit: header.gas_limit,
        });
    }
    Ok(())
}

/// Ensure the EIP-1559 base fee is set if the London hardfork is active.
#[inline]
pub fn validate_header_base_fee(
    header: &Header,
    chain_spec: &ChainSpec,
) -> Result<(), ConsensusError> {
    if chain_spec
        .is_fork_active_at_block(EthereumHardfork::London, header.number)
        && header.base_fee_per_gas.is_none()
    {
        return Err(ConsensusError::BaseFeeMissing);
    }
    Ok(())
}

/// Validate a block without regard for state:
///
/// - Compares the ommer hash in the block header to the block body
/// - Compares the transactions root in the block header to the block body
/// - Pre-execution transaction validation
/// - (Optionally) Compares the receipts root in the block header to the block body
pub fn validate_block_pre_execution(
    block: &SealedBlock,
    chain_spec: &ChainSpec,
) -> Result<(), ConsensusError> {
    // Check transaction root
    if let Err(error) = block.ensure_transaction_root_valid() {
        return Err(ConsensusError::BodyTransactionRootDiff(error.into()));
    }

    // EIP-4844: Shard Blob Transactions
    if chain_spec.is_cancun_active_at_timestamp(block.timestamp) {
        // Check that the blob gas used in the header matches the sum of the blob gas used by each
        // blob tx
        let header_blob_gas_used = block
            .blob_gas_used
            .ok_or(ConsensusError::BlobGasUsedMissing)?;
        let total_blob_gas = block.blob_gas_used().unwrap_or_default();
        if total_blob_gas != header_blob_gas_used {
            return Err(ConsensusError::BlobGasUsedDiff(GotExpected {
                got: header_blob_gas_used,
                expected: total_blob_gas,
            }));
        }
    }

    Ok(())
}

/// Validates that the EIP-4844 header fields exist and conform to the spec. This ensures that:
///
///  * `blob_gas_used` exists as a header field
///  * `excess_blob_gas` exists as a header field
///  * `parent_beacon_block_root` exists as a header field
///  * `blob_gas_used` is less than or equal to `MAX_DATA_GAS_PER_BLOCK`
///  * `blob_gas_used` is a multiple of `DATA_GAS_PER_BLOB`
///  * `excess_blob_gas` is a multiple of `DATA_GAS_PER_BLOB`
pub fn validate_4844_header_standalone(
    header: &Header,
) -> Result<(), ConsensusError> {
    let blob_gas_used = header
        .blob_gas_used
        .ok_or(ConsensusError::BlobGasUsedMissing)?;
    let excess_blob_gas = header
        .excess_blob_gas
        .ok_or(ConsensusError::ExcessBlobGasMissing)?;

    if header.parent_beacon_block_root.is_none() {
        return Err(ConsensusError::ParentBeaconBlockRootMissing);
    }

    if blob_gas_used > MAX_DATA_GAS_PER_BLOCK {
        return Err(ConsensusError::BlobGasUsedExceedsMaxBlobGasPerBlock {
            blob_gas_used,
            max_blob_gas_per_block: MAX_DATA_GAS_PER_BLOCK,
        });
    }

    if blob_gas_used % DATA_GAS_PER_BLOB != 0 {
        return Err(ConsensusError::BlobGasUsedNotMultipleOfBlobGasPerBlob {
            blob_gas_used,
            blob_gas_per_blob: DATA_GAS_PER_BLOB,
        });
    }

    // `excess_blob_gas` must also be a multiple of `DATA_GAS_PER_BLOB`. This will be checked later
    // (via `calculate_excess_blob_gas`), but it doesn't hurt to catch the problem sooner.
    if excess_blob_gas % DATA_GAS_PER_BLOB != 0 {
        return Err(ConsensusError::BlobGasUsedNotMultipleOfBlobGasPerBlob {
            blob_gas_used: excess_blob_gas,
            blob_gas_per_blob: DATA_GAS_PER_BLOB,
        });
    }

    Ok(())
}

/// Validates against the parent hash and number.
///
/// This function ensures that the header block number is sequential and that the hash of the parent
/// header matches the parent hash in the header.
#[inline]
pub fn validate_against_parent_hash_number(
    header: &Header,
    parent: &SealedHeader,
) -> Result<(), ConsensusError> {
    // Parent number is consistent.
    if parent.number + 1 != header.number {
        return Err(ConsensusError::ParentBlockNumberMismatch {
            parent_block_number: parent.number,
            block_number: header.number,
        });
    }

    if parent.hash() != header.parent_hash {
        return Err(ConsensusError::ParentHashMismatch(
            GotExpected {
                got: header.parent_hash,
                expected: parent.hash(),
            }
            .into(),
        ));
    }

    Ok(())
}

/// Validates the base fee against the parent and EIP-1559 rules.
#[inline]
pub fn validate_against_parent_eip1559_base_fee(
    header: &Header,
    parent: &Header,
    chain_spec: &ChainSpec,
) -> Result<(), ConsensusError> {
    if chain_spec
        .fork(EthereumHardfork::London)
        .active_at_block(header.number)
    {
        let base_fee = header
            .base_fee_per_gas
            .ok_or(ConsensusError::BaseFeeMissing)?;

        let expected_base_fee = if chain_spec
            .fork(EthereumHardfork::London)
            .transitions_at_block(header.number)
        {
            alloy_eips::eip1559::INITIAL_BASE_FEE
        } else {
            // This BaseFeeMissing will not happen as previous blocks are checked to have
            // them.
            parent
                .next_block_base_fee(
                    chain_spec.base_fee_params_at_timestamp(header.timestamp),
                )
                .ok_or(ConsensusError::BaseFeeMissing)?
        };
        if expected_base_fee != base_fee {
            return Err(ConsensusError::BaseFeeDiff(GotExpected {
                expected: expected_base_fee,
                got: base_fee,
            }));
        }
    }

    Ok(())
}

/// Validates the timestamp against the parent to make sure it is in the past.
#[inline]
pub const fn validate_against_parent_timestamp(
    header: &Header,
    parent: &Header,
) -> Result<(), ConsensusError> {
    if header.timestamp <= parent.timestamp {
        return Err(ConsensusError::TimestampIsInPast {
            parent_timestamp: parent.timestamp,
            timestamp: header.timestamp,
        });
    }
    Ok(())
}

/// Validates that the EIP-4844 header fields are correct with respect to the parent block. This
/// ensures that the `blob_gas_used` and `excess_blob_gas` fields exist in the child header, and
/// that the `excess_blob_gas` field matches the expected `excess_blob_gas` calculated from the
/// parent header fields.
pub fn validate_against_parent_4844(
    header: &Header,
    parent: &Header,
) -> Result<(), ConsensusError> {
    // From [EIP-4844](https://eips.ethereum.org/EIPS/eip-4844#header-extension):
    //
    // > For the first post-fork block, both parent.blob_gas_used and parent.excess_blob_gas
    // > are evaluated as 0.
    //
    // This means in the first post-fork block, calculate_excess_blob_gas will return 0.
    let parent_blob_gas_used = parent.blob_gas_used.unwrap_or(0);
    let parent_excess_blob_gas = parent.excess_blob_gas.unwrap_or(0);

    if header.blob_gas_used.is_none() {
        return Err(ConsensusError::BlobGasUsedMissing);
    }
    let excess_blob_gas = header
        .excess_blob_gas
        .ok_or(ConsensusError::ExcessBlobGasMissing)?;

    let expected_excess_blob_gas =
        calc_excess_blob_gas(parent_excess_blob_gas, parent_blob_gas_used);
    if expected_excess_blob_gas != excess_blob_gas {
        return Err(ConsensusError::ExcessBlobGasDiff {
            diff: GotExpected {
                got: excess_blob_gas,
                expected: expected_excess_blob_gas,
            },
            parent_excess_blob_gas,
            parent_blob_gas_used,
        });
    }

    Ok(())
}
