# QUAL-007: unreachable!() in RuntimeVersion::decompress is reachable on database corruption

## Finding

- **File**: `crates/botanix-storage/src/models/activation_manager.rs:101`
- **Severity**: Medium
- **Category**: Panic

The `Decompress` implementation for `RuntimeVersion` used `unreachable!()` when `value.len() < 4`. This is in the database deserialization path. If the database is corrupted, truncated, or a migration bug stores fewer than 4 bytes, the node panics instead of returning a `DatabaseError`.

## Status

**Confirmed** — the `unreachable!()` was present at line 101 on the current branch.

## Remediation

Replaced `unreachable!("passed on wrong value to decompress")` with:

```rust
return Err(
    reth_storage_errors::db::DatabaseError::Other(
        "RuntimeVersion value too short".into(),
    ),
);
```

This matches the audit recommendation exactly. The function already returns `Result<Self, DatabaseError>`, so callers naturally handle this error without any API change.

## Verification

- `cargo check -p botanix-storage` — compiles cleanly
- `cargo test -p botanix-storage --features test-utils --lib -- activation_manager` — all 7 tests pass
  - Includes new `test_runtime_version_decompress_too_short` covering inputs of 0, 1, 2, and 3 bytes
- Pre-existing issues in the crate (unrelated `test_utils` import in `snapshot.rs`, clippy errors in `botanix-primitives`) are not introduced by this change

## Files Changed

- `crates/botanix-storage/src/models/activation_manager.rs` — replaced `unreachable!()` with `DatabaseError::Other` return; added regression test
