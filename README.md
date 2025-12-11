# Reth @ Botanix

A Botanix-compatible Reth client implementation. This project is **not** a fork of Reth, but rather an extension that leverages Reth's powerful `NodeBuilder` API to provide Botanix chain compatibility.

## Note

This repository will supersede the [Macbeth](https://github.com/botanix-labs/Macbeth) repository which is currently deployed to mainnet. This `botanix` repository will be deployed to a new testnet then to mainnet. This is targeted for Q1 2026. Then the `Macbeth` repository will be deprecated.

## About

This project aims to bring Reth's high-performance Ethereum client capabilities to the Botanix L2 network. By utilizing Reth's modular architecture and NodeBuilder API, we're building a Botanix-compatible client that maintains compatibility with Reth's ecosystem while adding Botanix-specific features.

## Current Status

- Historical Sync ✅
- Pectra Support ✅
- Live Sync ✅

## Building

This project uses Rust `1.92.0`. The [`rust-toolchain.toml`](rust-toolchain.toml) file automatically configures the correct toolchain and components.

**First-time setup:**

```console
rustup install 1.92.0
```

The `rustfmt` and `clippy` components are installed automatically when the `cargo` command is run.

**Verify setup:**

```console
$ rustc --version
rustc 1.92.0 (ded5c06cf 2025-12-08)

$ cargo fmt --version
rustfmt 1.8.0-stable (ded5c06cf2 2025-12-08)

$ cargo clippy --version
clippy 0.1.92 (ded5c06cf2 2025-12-08)
```

## Getting Started

Refer to the [Reth documentation](https://reth.rs/) for general guidance on running a node and be sure to
add these 2 cli required to start botanix-reth:

```bash
cargo run --bin botanix-reth
--chain botanix-testnet \
    --db.max-size 7TB
```

## Contributing

Please feel free to open issues or submit pull requests.

## Disclaimer

This project is experimental and under active development. Use at your own risk.
