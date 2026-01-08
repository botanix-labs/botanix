//! Shared primitive types for Botanix blockchain.
//!
//! This crate provides common types used across multiple Botanix crates,
//! avoiding duplication and ensuring consistency.

mod multisig_id;

pub use multisig_id::{MultisigId, LEGACY_MULTISIG_ID, TEST_LEGACY_MULTISIG_ID, default_multisig_id};

