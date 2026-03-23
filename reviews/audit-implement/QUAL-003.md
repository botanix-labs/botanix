# QUAL-003: println! in production deserialization path

## Finding

A `println!("Error: {:?}", e)` call exists in `ExtraDataHeader::deserialize()` at
`crates/botanix-authority-edh/src/extra_data_header.rs:139`. This is a production
code path that deserializes the aggregate public key during block processing.
Raw `println!` bypasses the structured logging infrastructure (`tracing`), cannot
be filtered or routed, and violates the project's clippy policy that disallows
`log::*` macros in favor of `tracing`.

## Confirmed

The finding still applies on the current `main`-based code. The `println!` was
present at line 139 of `extra_data_header.rs`.

## Remediation

Replaced:

```rust
println!("Error: {:?}", e);
```

with:

```rust
tracing::error!("malformed aggregate public key: {:?}", e);
```

`tracing` was already a dependency of `botanix-authority-edh`. The new call:
- Uses the project-standard `tracing` framework
- Logs at `error` level (appropriate — a malformed aggregate public key is serious)
- Provides descriptive context instead of a bare "Error:"
- Respects log filtering and structured output

## Verification

- `cargo check -p botanix-authority-edh` — compiles cleanly
- `cargo nextest run -p botanix-authority-edh` — 6/6 tests pass
- `cargo clippy -p botanix-authority-edh --no-deps -- -D warnings` — no warnings
