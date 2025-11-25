//! Botanix Reth crate providing core modules and primitives.

/// Consensus module for consensus-related logic.
pub mod consensus;

mod evm;

/// Node module containing node primitives and logic.
pub mod node;
pub mod services;

pub use node::primitives::{BotanixBlock, BotanixBlockBody, BotanixPrimitives};
