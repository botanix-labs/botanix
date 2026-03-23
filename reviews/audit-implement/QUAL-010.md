# QUAL-010: ommer_reward has a latent subtraction underflow

## Finding

- **File**: `crates/botanix-consensus-common/src/calc.rs:122`
- **Severity**: Medium
- **Category**: Integer overflow / Underflow
- **Source**: `audit/final_reth_rc_audit.pdf` (page 26-27)

## Description

The expression `((8 + ommer_block_number - block_number) as u128 * base_block_reward) >> 3`
uses `u64` arithmetic. If `block_number > ommer_block_number + 8`, the subtraction wraps to
a very large `u64`, producing an astronomically large reward. While Botanix is post-merge
and does not use ommers, the function exists in the public library API.

## Applicability

**Still applies.** The function existed at the audited location with the exact vulnerable
expression prior to this fix.

## Remediation

Changed `ommer_reward` return type from `u128` to `Option<u128>`. Replaced the raw
subtraction with `checked_sub`:

```rust
match (8 + ommer_block_number).checked_sub(block_number) {
    Some(numerator) => Some((numerator as u128 * base_block_reward) >> 3),
    None => None,
}
```

When the inputs would cause underflow, the function now returns `None` instead of silently
wrapping. This follows the audit suggestion to use `checked_sub` and return `Option<u128>`.

No callers exist in the current codebase, so the signature change has zero downstream impact.

## Verification

- `cargo nextest run -p botanix-consensus-common` — 8/8 tests pass
- New `calc_ommer_reward` test covers: valid reward, same-block ommer, underflow (returns
  `None`), and zero-numerator edge case
- `cargo fmt` clean
- Pre-existing clippy warnings in the crate are unrelated (unused imports, missing const)
