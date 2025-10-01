use revm::precompile::PrecompileError;

/// Botanix specific precompile errors.
#[derive(Debug, PartialEq)]
pub enum BotanixPrecompileError {
    /// The cometbft validation input is invalid.
    InvalidInput,
    /// The cometbft apply block failed.
    CometBftApplyBlockFailed,
    /// The cometbft consensus state encoding failed.
    CometBftEncodeConsensusStateFailed,
    /// The double sign invalid evidence.
    DoubleSignInvalidEvidence,
}

impl From<BotanixPrecompileError> for PrecompileError {
    fn from(error: BotanixPrecompileError) -> Self {
        match error {
            BotanixPrecompileError::InvalidInput => PrecompileError::Other("invalid input".to_string()),
            BotanixPrecompileError::CometBftApplyBlockFailed => {
                PrecompileError::Other("apply block failed".to_string())
            }
            BotanixPrecompileError::CometBftEncodeConsensusStateFailed => {
                PrecompileError::Other("encode consensus state failed".to_string())
            }
            BotanixPrecompileError::DoubleSignInvalidEvidence => {
                PrecompileError::Other("double sign invalid evidence".to_string())
            }
        }
    }
}
