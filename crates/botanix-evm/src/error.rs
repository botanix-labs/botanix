/// `BlockExecutor` Errors
use derive_more::Display;
use reth_evm::{
    block::{BlockValidationError, InternalBlockExecutionError},
    revm::primitives::{
        alloy_primitives::{BlockHash, BlockNumber, Bloom},
        B256,
    },
};
use reth_primitives::{GotExpected, GotExpectedBoxed, InvalidTransactionError};
use reth_primitives_traits::constants::MINIMUM_GAS_LIMIT;
use reth_storage_errors::provider::ProviderError;

#[derive(Debug, Display)]
pub enum BlockExecutionError {
    /// Validatidisplayr, transparently wrapping [`BlockValidationError`]
    Validation(BlockValidationError),
    /// Consensus error, transparently wrapping [`ConsensusError`]
    Consensus(ConsensusError),
    /// Internal, i.e. non consensus or validation related Block Executor Errors
    Internal(InternalBlockExecutionError),
}

// TODO: this enum has a mix of concerns and should be broken into separate enums
/// For example, BotanixConsensusError
/// Consensus Errors
#[derive(Debug, Clone, derive_more::Display)]
pub enum ConsensusError {
    /// Error when the gas used in the header exceeds the gas limit.
    #[display("block used gas ({gas_used}) is greater than gas limit ({gas_limit})")]
    HeaderGasUsedExceedsGasLimit {
        /// The gas used in the block header.
        gas_used: u64,
        /// The gas limit in the block header.
        gas_limit: u64,
    },
    /// `PoA` specific: missing quorum of authority signatures
    #[display("Missing quorum of authority signatures, expected: {expected}, got: {actual}")]
    MissingQuorumOfAuthoritySignatures {
        /// The expected quorum of authority signatures.
        expected: u16,
        /// The actual quorum of authority signatures.
        actual: u16,
    },
    /// `PoA` specific: authority list is missing in the extra data header
    #[display("Missing authority list")]
    MissingAuthorityList,
    /// `PoA` specific: authority list does not match the genesis block authority list
    #[display("Invalid authority list")]
    InvalidAuthorityList,
    /// `PoA` specific: Invalid block signature
    #[display("Invalid authority signature")]
    InvalidAuthoritySignature,
    /// `PoA` specific: extra data header does not follow consensus rules or is malformed
    #[display("Invalid extra data")]
    ExtraDataInvalid,
    /// `PoA` specific: failed to recover authority from block signature
    #[display("Failed to recover authority")]
    FailedToRecoverAuthority,
    /// `PoA` specific: same signer as previous block
    #[display("Same signer as previous block")]
    SignerLimitExceeded,
    /// `PoA` specific: authority signer not in turn
    #[display("Authority signer not in turn")]
    AuthorityNotInTurn,
    /// `PoA` specific: block beneficiary is not an authority
    #[display("block beneficiary is not an authority")]
    BlockBeneficiaryIsNotAuthority,
    /// `PoA` specific: block beneficiary is not the burn address
    #[display("block beneficiary is not the burn address")]
    BlockBeneficiaryIsNotBurnAddress,

    /// Error when block gas used doesn't match expected value
    #[display(
        "block gas used mismatch: {gas}; gas spent by each transaction: {gas_spent_by_tx:?}"
    )]
    BlockGasUsed {
        /// The gas diff.
        gas: GotExpected<u64>,
        /// Gas spent by each transaction
        gas_spent_by_tx: Vec<(u64, u64)>,
    },

    /// Error when the hash of block ommer is different from the expected hash.
    #[display("mismatched block ommer hash: {_0}")]
    BodyOmmersHashDiff(GotExpectedBoxed<B256>),

    /// Error when the state root in the block is different from the expected state root.
    #[display("mismatched block state root: {_0}")]
    BodyStateRootDiff(GotExpectedBoxed<B256>),

    /// Error when the transaction root in the block is different from the expected transaction
    /// root.
    #[display("mismatched block transaction root: {_0}")]
    BodyTransactionRootDiff(GotExpectedBoxed<B256>),

    /// Error when the receipt root in the block is different from the expected receipt root.
    #[display("receipt root mismatch: {_0}")]
    BodyReceiptRootDiff(GotExpectedBoxed<B256>),

    /// Error when header bloom filter is different from the expected bloom filter.
    #[display("header bloom filter mismatch: {_0}")]
    BodyBloomLogDiff(GotExpectedBoxed<Bloom>),

    /// Error when the withdrawals root in the block is different from the expected withdrawals
    /// root.
    #[display("mismatched block withdrawals root: {_0}")]
    BodyWithdrawalsRootDiff(GotExpectedBoxed<B256>),

    /// Error when the requests root in the block is different from the expected requests
    /// root.
    #[display("mismatched block requests root: {_0}")]
    BodyRequestsRootDiff(GotExpectedBoxed<B256>),

    /// Error when a block with a specific hash and number is already known.
    #[display("block with [hash={hash}, number={number}] is already known")]
    BlockKnown {
        /// The hash of the known block.
        hash: BlockHash,
        /// The block number of the known block.
        number: BlockNumber,
    },

    /// Error when the parent hash of a block is not known.
    #[display("block parent [hash={hash}] is not known")]
    ParentUnknown {
        /// The hash of the unknown parent block.
        hash: BlockHash,
    },

    /// Error when the block number does not match the parent block number.
    #[display(
        "block number {block_number} does not match parent block number {parent_block_number}"
    )]
    ParentBlockNumberMismatch {
        /// The parent block number.
        parent_block_number: BlockNumber,
        /// The block number.
        block_number: BlockNumber,
    },

    /// Error when the parent hash does not match the expected parent hash.
    #[display("mismatched parent hash: {_0}")]
    ParentHashMismatch(GotExpectedBoxed<B256>),

    /// Error when the block timestamp is in the future compared to our clock time.
    #[display(
        "block timestamp {timestamp} is in the future compared to our clock time {present_timestamp}"
    )]
    TimestampIsInFuture {
        /// The block's timestamp.
        timestamp: u64,
        /// The current timestamp.
        present_timestamp: u64,
    },

    /// Error when the block timestamp is in the past compared to our clock time.
    #[display(
        "block timestamp {timestamp} is in the past compared to our clock time {present_timestamp}"
    )]
    TimestampIsInPast {
        /// The block's timestamp.
        timestamp: u64,
        /// The current timestamp.
        present_timestamp: u64,
    },

    /// Error when the base fee is missing.
    #[display("base fee missing")]
    BaseFeeMissing,

    /// Error when there is a transaction signer recovery error.
    #[display("transaction signer recovery error")]
    TransactionSignerRecoveryError,

    /// Error when the extra data length exceeds the maximum allowed.
    #[display("extra data {len} exceeds max length")]
    ExtraDataExceedsMax {
        /// The length of the extra data.
        len: usize,
    },

    /// Error when the extra data header length exceeds the maximum allowed.
    #[display("extra data header {len} exceeds max length")]
    ExtraDataHeaderExceedsMax {
        /// The length of the extra header data.
        len: usize,
    },

    /// Consensus error during PBFT
    #[display("PBFT Consensus error")]
    PBFTConsensusError,

    /// Error when the difficulty after a merge is not zero.
    #[display("difficulty after merge is not zero")]
    TheMergeDifficultyIsNotZero,

    /// Error when the nonce after a merge is not zero.
    #[display("nonce after merge is not zero")]
    TheMergeNonceIsNotZero,

    /// Error when the ommer root after a merge is not empty.
    #[display("ommer root after merge is not empty")]
    TheMergeOmmerRootIsNotEmpty,

    /// Error when the withdrawals root is missing.
    #[display("missing withdrawals root")]
    WithdrawalsRootMissing,

    /// Error when the requests root is missing.
    #[display("missing requests root")]
    RequestsRootMissing,

    /// Error when an unexpected withdrawals root is encountered.
    #[display("unexpected withdrawals root")]
    WithdrawalsRootUnexpected,

    /// Error when an unexpected requests root is encountered.
    #[display("unexpected requests root")]
    RequestsRootUnexpected,

    /// Error when withdrawals are missing.
    #[display("missing withdrawals")]
    BodyWithdrawalsMissing,

    /// Error when requests are missing.
    #[display("missing requests")]
    BodyRequestsMissing,

    /// Error when blob gas used is missing.
    #[display("missing blob gas used")]
    BlobGasUsedMissing,

    /// Error when unexpected blob gas used is encountered.
    #[display("unexpected blob gas used")]
    BlobGasUsedUnexpected,

    /// Error when excess blob gas is missing.
    #[display("missing excess blob gas")]
    ExcessBlobGasMissing,

    /// Error when unexpected excess blob gas is encountered.
    #[display("unexpected excess blob gas")]
    ExcessBlobGasUnexpected,

    /// Error when the parent beacon block root is missing.
    #[display("missing parent beacon block root")]
    ParentBeaconBlockRootMissing,

    /// Error when an unexpected parent beacon block root is encountered.
    #[display("unexpected parent beacon block root")]
    ParentBeaconBlockRootUnexpected,

    /// Error when blob gas used exceeds the maximum allowed.
    #[display("blob gas used {blob_gas_used} exceeds maximum allowance {max_blob_gas_per_block}")]
    BlobGasUsedExceedsMaxBlobGasPerBlock {
        /// The actual blob gas used.
        blob_gas_used: u64,
        /// The maximum allowed blob gas per block.
        max_blob_gas_per_block: u64,
    },

    /// Error when blob gas used is not a multiple of blob gas per blob.
    #[display(
        "blob gas used {blob_gas_used} is not a multiple of blob gas per blob {blob_gas_per_blob}"
    )]
    BlobGasUsedNotMultipleOfBlobGasPerBlob {
        /// The actual blob gas used.
        blob_gas_used: u64,
        /// The blob gas per blob.
        blob_gas_per_blob: u64,
    },

    /// Error when excess blob gas is not a multiple of blob gas per blob.
    #[display(
        "excess blob gas {excess_blob_gas} is not a multiple of blob gas per blob {blob_gas_per_blob}"
    )]
    ExcessBlobGasNotMultipleOfBlobGasPerBlob {
        /// The actual excess blob gas.
        excess_blob_gas: u64,
        /// The blob gas per blob.
        blob_gas_per_blob: u64,
    },

    /// Error when the blob gas used in the header does not match the expected blob gas used.
    #[display("blob gas used mismatch: {_0}")]
    BlobGasUsedDiff(GotExpected<u64>),

    /// Error for a transaction that violates consensus.
    InvalidTransaction(InvalidTransactionError),

    // /// Error type transparently wrapping HeaderValidationError.
    // #[display("failed to validate header: {_0}")]
    // HeaderValidationError(HeaderValidationError),
    /// Inturn Validation Error
    #[display("in turn validation error")]
    ValidateInturnError,

    /// Invalid Aggregated Public key
    #[display("invalid aggregated public key: {_0}")]
    InvalidAggregatedPublicKey(InvalidAggregatedPublicKeyError),

    /// Invalid Chain Version
    #[display("invalid chain version")]
    InvalidChainVersion,

    /// Error when the block's base fee is different from the expected base fee.
    #[display("block base fee mismatch: {_0}")]
    BaseFeeDiff(GotExpected<u64>),

    /// Error when there is an invalid excess blob gas.
    #[display(
        "invalid excess blob gas: {diff}; \
            parent excess blob gas: {parent_excess_blob_gas}, \
            parent blob gas used: {parent_blob_gas_used}"
    )]
    ExcessBlobGasDiff {
        /// The excess blob gas diff.
        diff: GotExpected<u64>,
        /// The parent excess blob gas.
        parent_excess_blob_gas: u64,
        /// The parent blob gas used.
        parent_blob_gas_used: u64,
    },

    /// Error when the child gas limit exceeds the maximum allowed increase.
    #[display("child gas_limit {child_gas_limit} max increase is {parent_gas_limit}/1024")]
    GasLimitInvalidIncrease {
        /// The parent gas limit.
        parent_gas_limit: u64,
        /// The child gas limit.
        child_gas_limit: u64,
    },

    /// Error indicating that the child gas limit is below the minimum allowed limit.
    ///
    /// This error occurs when the child gas limit is less than the specified minimum gas limit.
    #[display(
        "child gas limit {child_gas_limit} is below the minimum allowed limit ({MINIMUM_GAS_LIMIT})"
    )]
    GasLimitInvalidMinimum {
        /// The child gas limit.
        child_gas_limit: u64,
    },

    /// Error when the child gas limit exceeds the maximum allowed decrease.
    #[display("child gas_limit {child_gas_limit} max decrease is {parent_gas_limit}/1024")]
    GasLimitInvalidDecrease {
        /// The parent gas limit.
        parent_gas_limit: u64,
        /// The child gas limit.
        child_gas_limit: u64,
    },

    /// Cannot add and existing federation member to the federation
    #[display("Cannot add and existing federation member to the federation")]
    CannotAddExistingFederationMember,

    /// Error deserializing extra data header.
    #[display("error deserializing extra data header")]
    NonDeterministicDataDeserialize,

    /// Error since the latest header is missing.
    #[display("missing latest header")]
    LatestHeaderMissing,

    /// Error from provider.
    #[display("provider error: {_0}")]
    Provider(ProviderError),

    /// Error since the bitcoin checkpoint is missing.
    #[display("missing latest header")]
    MissingBitcoinCheckpoint,

    /// Error since the block fee recipient fee is missing.
    #[display("missing block fee recipient address")]
    MissingBlockFeeRecipientAddress,
}

/// Invalid Aggregated Public Key Error
#[derive(Debug, PartialEq, Eq, Clone, derive_more::Display)]
pub enum InvalidAggregatedPublicKeyError {
    /// Aggregated public key does not match expected key
    #[display("Aggregated public key does not match expected key")]
    InvalidAggregatedPublicKey,
    /// Aggregated public key is missing
    #[display("Aggregated public key is missing")]
    MissingAggregatedPublicKey,
    /// Aggregated public key should not be NUMS point past genesis block
    #[display("Aggregated public key should not be NUMS point past genesis block")]
    NumsAggregatePublicKeyPastGenesis,
}
