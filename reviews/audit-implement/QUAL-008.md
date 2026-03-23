# QUAL-008: unreachable!() in NDD serialization for unknown versions

## Finding

- **File**: `bin/botanix-reth/src/consensus/comet_bft/non_deterministic_data.rs:200`
- **Severity**: Medium
- **Category**: Panic
- **Source**: `final_reth_rc_audit.pdf`

## Description

The `NonDeterministicData::serialize()` method used `unreachable!()` in its
version match arm for unknown versions. If a new NDD version were added to the
enum without updating this match, the node would crash at runtime instead of
returning a descriptive error.

## Verification

Confirmed the issue still exists on the current branch. The `deserialize()`
method already correctly returns
`Err(NonDeterministicDataDeserializeError::InvalidVersion)` for unknown
versions (line 286), making the `serialize()` inconsistency clear.

## Remediation

Replaced `unreachable!()` with a `return Err(io::Error::new(...))` that returns
`io::ErrorKind::InvalidData` with a descriptive message. This is consistent with
the function's existing `Result<Vec<u8>, io::Error>` return type and mirrors
how `deserialize()` handles unknown versions.

Added test `test_serialize_unknown_version_returns_error` that constructs an NDD
with an invalid version (99) and asserts that `serialize()` returns the expected
`InvalidData` error instead of panicking.

## Test Results

All 7 NDD tests pass including the new test:
- `test_non_deterministic_data_new` — ok
- `test_non_deterministic_data_new_v1` — ok
- `test_non_deterministic_data_new_v2` — ok
- `test_non_deterministic_data_serde_v0` — ok
- `test_non_deterministic_data_serde_v1` — ok
- `test_non_deterministic_data_serde_v2` — ok
- `test_serialize_unknown_version_returns_error` — ok
