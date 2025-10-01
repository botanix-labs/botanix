//! Botanix Reth crate providing core modules and primitives.

/// Chainspec module for chain specification definitions.
pub mod chainspec;

/// Consensus module for consensus-related logic.
pub mod consensus;

mod evm;
mod hardforks;

/// Node module containing node primitives and logic.
pub mod node;

pub use node::primitives::{BotanixBlock, BotanixBlockBody, BotanixPrimitives};
mod system_contracts;
