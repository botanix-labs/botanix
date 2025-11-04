#!/bin/bash

# Ensure pre-commit is installed, install via brew if missing, then install hooks
if ! command -v pre-commit &> /dev/null; then
    brew install pre-commit
fi
pre-commit install

# Install Bun as package manager for NodeJS if it doesn't exist
if ! command -v bun &> /dev/null; then
    curl -fsSL https://bun.sh/install | bash
fi

bun install

install_cmd="cargo binstall --force --no-confirm"

# Install cargo global crates
cargo install cargo-binstall
$install_cmd cargo-tarpaulin
$install_cmd samply
$install_cmd cargo-watch
$install_cmd knope
$install_cmd sqlx-cli
$install_cmd cargo-sort
$install_cmd typos-cli
$install_cmd cargo-nextest --secure

# Binstall does not support --features
cargo install cargo-audit --locked --features=fix --force
cargo install release-plz --locked
cargo install taplo-cli --locked
cargo install bacon --locked
cargo install cargo-machete --locked
