//! Crate-level documentation for botanix-evm.
//!
//! This crate provides EVM-related functionality for the Botanix project.

pub mod error;
/// Module containing the EVM executor implementation.
pub mod execute;

/// Utility functions for transaction handling and classification.
pub mod utils;

/// Module for state management and manipulation within the EVM context.
pub mod state;

/// Module for building execution payloads.
pub mod payload;
