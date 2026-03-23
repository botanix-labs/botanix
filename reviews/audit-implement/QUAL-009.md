# QUAL-009: TOCTOU race in `is_migration_needed` / `path_has_content`

## Finding

**File:** `crates/botanix-storage-migrate/src/migrate.rs:42-50`
**Severity:** Medium
**Category:** Storage correctness / Race condition
**Source:** `final_reth_rc_audit.pdf`

## Description

`path_has_content` checks `path.exists()` followed by `path.read_dir()` in two
separate filesystem operations. Between these calls, another process could create
or delete the directory. Additionally, `entries.count()` consumes the `ReadDir`
iterator without checking individual entry errors -- if a permission error occurs
reading an entry, it is silently skipped, potentially reporting a non-empty
directory as empty and skipping a needed migration.

## Status: Confirmed and Fixed

The finding was confirmed in the current codebase at the exact location
specified. The original code had both issues described.

## Remediation

Replaced the two-step `exists()` + `read_dir()` pattern with a single
`read_dir()` call that handles `NotFound` via error kind matching, eliminating
the TOCTOU race. Replaced `entries.count()` with an explicit iteration that
propagates per-entry errors via `?` and returns `true` on the first successful
entry, avoiding both the silent error swallowing and unnecessary full iteration.

## Verification

- All 7 existing `is_migration_needed` tests pass
- `cargo clippy -p botanix-storage-migrate -- -D warnings` clean (no warnings
  for this crate)
