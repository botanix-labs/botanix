# QUAL-006: `let _ = writer.write(...)` silently discards partial write in EDH serializer

## Finding

- **File**: `crates/botanix-authority-edh/src/extra_data_header.rs:106`
- **Severity**: High
- **Category**: Error propagation / Silent failure

In `encode_into_without_signature`, the 20-byte Ethereum address was written via
`let _ = writer.write(&block_producer_address_bytes)?`. While the `?` propagates
`io::Error`, `write()` is not guaranteed to write all bytes — it returns the
number of bytes actually written, which was discarded with `let _`. For in-memory
`Vec` buffers this is fine, but for any streaming writer the address could be
silently truncated, producing a structurally invalid EDH.

## Remediation

Replaced `writer.write()` with `writer.write_all()`, which guarantees all bytes
are written or returns an error.

```diff
- let _ = writer.write(&block_producer_address_bytes)?;
+ writer.write_all(&block_producer_address_bytes)?;
```

## Verification

- `cargo check -p botanix-authority-edh` — compiles cleanly
- `cargo nextest run -p botanix-authority-edh` — 6/6 tests pass, including
  `serialize_without_signature` which directly exercises the changed code path
