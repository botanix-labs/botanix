use bitcoin::hashes::{sha256, Hash};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigHashError {
    #[error("provided federation config hash must not be empty")]
    EmptyHash,
    #[error(
        "federation config hash mismatch: expected {expected}, found {found}"
    )]
    HashMismatch { expected: String, found: String },
}

pub fn verify_config_hash(
    raw: &str,
    expected_hash: &str,
) -> Result<(), ConfigHashError> {
    let normalized_expected = normalize_hash(expected_hash);
    if normalized_expected.is_empty() {
        return Err(ConfigHashError::EmptyHash);
    }

    let computed = compute_config_hash(raw);
    if normalized_expected != computed {
        return Err(ConfigHashError::HashMismatch {
            expected: normalized_expected,
            found: computed,
        });
    }

    Ok(())
}

pub fn compute_config_hash(raw: &str) -> String {
    sha256::Hash::hash(raw.as_bytes()).to_string()
}

pub fn normalize_hash(value: &str) -> String {
    value.trim().trim_start_matches("0x").to_ascii_lowercase()
}
