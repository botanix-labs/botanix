# Botanix Reth - Claude Code Configuration

## Project Overview

Botanix-compatible **Reth client extension** built using Reth's NodeBuilder API. This is NOT a fork — it extends Reth to provide Botanix chain compatibility with Bitcoin integration, CometBFT consensus, and FROST threshold signatures.

- **Language**: Rust (edition 2021, MSRV 1.90)
- **Build System**: Cargo workspace (35+ crates)
- **Formatter**: `.rustfmt.toml` (80 char width, 4 space indent)
- **License**: MIT OR Apache-2.0
- **Package Manager** (JS tooling): pnpm

---

## Commands

```yaml
# Build
install: "make install" # Install botanix-reth to ~/.cargo/bin
install-btc-server: "make install-btc-server" # Install btc-server
build: "make build" # Release build
build-debug: "make build-debug" # Debug build

# Testing
test-unit: "make test-unit" # Unit tests with nextest
test: "make test" # All tests
cov-unit: "make cov-unit" # Coverage with llvm-cov
cov-report-html: "make cov-report-html" # HTML coverage report

# Formatting & Linting
fmt: "make fmt" # Format all (Rust + TOML + Prettier + Markdown)
fmt-cargo: "make fmt-cargo" # Format TOML files (taplo)
fmt-rust: "make fmt-rust" # Format Rust code (rustfmt)
fmt-prettier: "make fmt-prettier" # Format JSON/MD/SH
fmt-markdown: "make fmt-markdown" # Format markdown
lint: "make lint" # Run all linting
lint-cargo: "make lint-cargo" # TOML validation
lint-rust: "make lint-rust" # Rust checks + fmt validation
lint-clippy: "make lint-clippy" # Clippy (-D warnings)
lint-prettier: "make lint-prettier" # Prettier validation
lint-markdown: "make lint-markdown" # Markdown linting
lint-machete: "make lint-machete" # Unused dependency check

# Documentation
docs: "make docs" # Generate docs
docs-serve: "make docs-serve" # Serve on localhost:8000

# Auditing
audit: "make audit" # Cargo audit
audit-fix: "make audit-fix" # Auto-fix vulnerabilities

# Docker (local dev)
init-docker-local: "make init-docker-local" # Initialize local docker
start-docker-local: "make start-docker-local" # Start local network
stop-docker-local: "make stop-docker-local" # Stop local network
reset-docker-local: "make reset-docker-local" # Reset data
clean-docker-local: "make clean-docker-local" # Full cleanup
```

---

## Crate Structure

### Binaries (`bin/`)

| Binary                   | Description                              |
| ------------------------ | ---------------------------------------- |
| `botanix-reth`           | Main EVM node (default workspace member) |
| `botanix-btc-server`     | Bitcoin server for federation/multisig   |
| `botanix-cli`            | CLI utility (binary: `btx`)              |
| `botanix-db-cleaner`     | Database cleanup utility                 |
| `botanix-up`             | Network initialization/setup             |
| `botanix-test-suite`     | Integration and E2E test suite           |
| `botanix-pegin-recovery` | Peg-in recovery utility                  |

### Libraries (`crates/`)

| Crate                        | Purpose                               |
| ---------------------------- | ------------------------------------- |
| `botanix-activation-manager` | Protocol activation phase management  |
| `botanix-authority-edh`      | ECDH for authorities                  |
| `botanix-authority-metrics`  | Authority metrics collection          |
| `botanix-authority-peg`      | Peg-in/out mechanisms                 |
| `botanix-authority-rsp`      | Remote signing protocol               |
| `botanix-bitcoin-checkpoint` | Bitcoin checkpoint tracking           |
| `botanix-btc-wallet`         | Bitcoin wallet implementations        |
| `botanix-chainspec`          | Chain specification and configuration |
| `botanix-cli-args`           | CLI argument definitions              |
| `botanix-cli-parsers`        | CLI argument parsing                  |
| `botanix-comet-bft-rpc`      | CometBFT RPC client integration       |
| `botanix-configs`            | Configuration management              |
| `botanix-consensus-common`   | Common consensus types/functions      |
| `botanix-data-parser`        | Data parsing and compression          |
| `botanix-evm`                | Custom EVM implementations            |
| `botanix-fs-util`            | File system utilities                 |
| `botanix-primitives`         | Core blockchain primitives            |
| `botanix-rpc-client`         | RPC client functionality              |
| `botanix-rpc-config`         | RPC configuration                     |
| `botanix-rpc-types`          | RPC type definitions                  |
| `botanix-storage`            | Storage layer abstraction             |
| `botanix-storage-migrate`    | DB migration from Reth to Botanix     |
| `botanix-types`              | Shared primitive types                |
| `botanix-utils`              | General utilities                     |

### Smart Contracts (`contracts/`)

Solidity contracts built with **Foundry**. System contracts embedded at build time via `build.rs` for multiple hardforks.

---

## Code Style & Linting Rules

### Rust Formatting (`.rustfmt.toml`)

- Max width: **80 characters**
- Tab spaces: 4
- Reorder imports/modules: enabled
- Use field init shorthand and try shorthand

### Clippy (`.clippy.toml`)

- **Use `tracing`** — `log::*` macros are disallowed
- **No `todo!()`, `dbg!()`, `unimplemented!()`** in committed code
- **No `for_each`/`try_for_each`** — use `for` loops for side-effects
- `unwrap`/`expect`/`dbg`/`print`/`panic` allowed **only in tests**
- Large error threshold: 512 bytes

### Workspace Lints (`Cargo.toml`)

- `rust_2018_idioms` = **deny**
- `unused_must_use` = **deny**
- `missing_docs` = warn
- `unreachable_pub` = warn
- 40+ clippy nursery lints enabled

### Commenting Guidelines

Write comments that remain valuable after a PR is merged. Future readers won't have PR context — they only see the current code.

**DO: Explain WHY and non-obvious behavior**

```rust
// Process must handle allocations atomically to prevent race conditions
// between dealloc on drop and concurrent limit checks
unsafe impl GlobalAlloc for LimitedAllocator { ... }

// Timeout set to 5s to match EVM block processing limits
const TRACER_TIMEOUT: Duration = Duration::from_secs(5);
```

**DO: Document constraints and assumptions**

```rust
/// Returns heap size estimate.
///
/// Note: May undercount shared references (Rc/Arc). For precise
/// accounting, combine with an allocator-based approach.
fn deep_size_of(&self) -> usize
```

**DON'T: Describe changes or restate code**

```rust
// BAD - Describes the change, not the code
// Changed from Vec to HashMap for O(1) lookups

// GOOD - Explains the decision
// HashMap provides O(1) symbol lookups during trace replay
```

```rust
// BAD - PR-specific context
// Fix for issue #234 where memory wasn't freed

// GOOD - Documents the actual behavior
// Explicitly drop allocations before limit check to ensure
// accurate accounting
```

**Comment when:**

- Non-obvious behavior or edge cases
- Performance trade-offs
- Safety requirements (`unsafe` blocks must always be documented)
- Limitations or gotchas
- Why simpler alternatives don't work

**Don't comment when:**

- Code is self-explanatory
- Just restating the code in English
- Describing what changed in a PR

**The test:** "Will this make sense in 6 months without PR context?"

### Type Ordering in Files

The file's primary type (matching the file name) comes first, followed by supporting public types, then private types and helpers.

```rust
use ...;

/// The primary type of this file (matches filename).
pub struct PayloadProcessor { ... }

impl PayloadProcessor { ... }

// Public auxiliary types that support the primary type
pub struct PayloadProcessorConfig { ... }

// Public traits related to the primary type
pub trait ProcessorExt { ... }

// Private helper types
struct InternalState { ... }

// Private helper functions
fn validate_input() { ... }
```

### Performance Considerations

1. **Avoid allocations in hot paths** — use references and borrowing
2. **Parallel processing** — use rayon for CPU-bound parallel work
3. **Async/await** — use tokio for I/O-bound operations
4. **Don't block async tasks** — use `spawn_blocking` for CPU-intensive work or work with lots of blocking I/O
5. **Handle errors properly** — use `?` operator and proper error types

---

## Pre-commit Hooks

Hooks run automatically on commit:

| Hook                      | Description                    |
| ------------------------- | ------------------------------ |
| `format`                  | Runs `make fmt`                |
| `trailing-whitespace`     | Remove trailing whitespace     |
| `end-of-file-fixer`       | Ensure files end with newline  |
| `check-json`              | Validate JSON syntax           |
| `check-toml`              | Validate TOML syntax           |
| `check-added-large-files` | Block files > 3MB              |
| `check-merge-conflict`    | Detect merge conflict markers  |
| `check-case-conflict`     | Detect filename case conflicts |
| `detect-private-key`      | Scan for private keys          |
| `yamlfix`                 | Auto-format YAML files         |

Run manually: `pre-commit run --all-files`

---

## Workflow: Reviewing Branch Changes

When reviewing changes in a branch, run these checks (in parallel where possible):

1. **Check for silent failures** — swallowed errors, missing error handling
2. **Verify code comments** — comments match implementation
3. **Review new types** — ensure types are well-designed
4. **General code review** — logic, security, best practices
5. **Formatting** — `make fmt` (fix any issues)
6. **Linting** — `make lint`
7. **Tests** — `make test-unit`
8. **Write PR summary** — short summary of important changes

### Review Checklist

```
[ ] Silent failures checked
[ ] Comments accurate
[ ] New types reviewed
[ ] General code review complete
[ ] make fmt (no changes needed)
[ ] make lint passes
[ ] Relevant tests pass
[ ] PR summary written
```

---

## Validation

For Rust code changes:

```bash
make fmt && make lint && make test-unit
```

Quick check (no tests):

```bash
cargo check --workspace && make lint-clippy
```

For smart contract changes:

```bash
cd contracts && forge build && forge test
```

---

## Key Dependencies

- **Reth**: Custom fork from `botanix-labs/reth` (branch `dynafed`)
- **Alloy**: Ethereum types and RLP encoding
- **Bitcoin**: `bitcoin`, `bitcoincore-rpc`, `miniscript`, `bdk_wallet`
- **Cryptography**: FROST secp256k1, BLS on Arkworks
- **Consensus**: CometBFT / Tendermint
- **Async**: Tokio, futures
- **RPC**: jsonrpsee
- **Metrics**: Prometheus

---

## Build Profiles

| Profile     | Description                               |
| ----------- | ----------------------------------------- |
| `dev`       | Default debug build                       |
| `release`   | opt-level=3, thin LTO, 16 codegen-units   |
| `maxperf`   | Fat LTO, 1 codegen-unit (max performance) |
| `profiling` | Release with full debug symbols           |
| `hivetests` | Test profile with opt-level=3             |

---

## Security

- **Never commit `.env` files** — use `.env.sample` as reference
- **Private keys** — stored in `.env`, never hardcoded
- **Pre-commit hooks** scan for secrets and private keys
- **`openssl` crate is denied** — see `deny.toml`
- **Cargo audit** — run `make audit` to check for vulnerabilities
