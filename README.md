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

## Getting Started

Refer to the [Reth documentation](https://reth.rs/) for general guidance on running a node and be sure to
add these 2 cli required to start botanix-reth:

```bash
cargo run --bin botanix-reth
--chain botanix-testnet \
    --db.max-size 7TB
```

## Development Setup

### Prerequisites

Run the setup script to install all required tools (pre-commit, bun, cargo tools):

```bash
./scripts/setup.sh
```

### Build & Test

```bash
make build       # Release build
make build-debug # Debug build
make test-unit   # Run unit tests (nextest)
make fmt         # Format everything (Rust + TOML + Prettier + Markdown)
make lint        # Run all linters
```

### Local Docker Environment

```bash
make init-docker-local  # Initialize local docker setup
make start-docker-local # Start local network
make stop-docker-local  # Stop local network
```

See [docs/local_setup.md](docs/local_setup.md) for detailed instructions.

## AI Agent Skills

This project supports AI coding agents (Claude Code, Codex, Cursor, etc.) via shared skills from [botanix-labs/botanix-skills](https://github.com/botanix-labs/botanix-skills).

### Installing Skills

Install skills for all supported agents:

```bash
./scripts/update-agents.sh
```

Or for a specific agent only:

```bash
./scripts/update-agents.sh claude-code
./scripts/update-agents.sh codex
```

This installs skills into `.agents/skills/` with symlinks in each agent's config directory (e.g., `.claude/skills/`).

### Claude Code Setup

The project includes Claude Code configuration out of the box:

- `CLAUDE.md` + `PROJECT.md` — project knowledge and workflow instructions
- `.claude/settings.json` — shared plugins and permissions
- `.claude/settings.local.json` — personal overrides (not committed)

#### MCP Servers

External tool integrations are configured in `.mcp.json` (GitHub, Linear, Sentry, and Google Cloud).

**GitHub** — requires a [Personal Access Token](https://github.com/settings/tokens) with `repo`, `issues`, and `project` scopes.

Add both to your shell profile (`~/.bashrc` or `~/.zshrc`):

```bash
export GITHUB_PERSONAL_ACCESS_TOKEN="ghp_your_token_here"
```

Then restart your terminal and Claude Code. Tokens are picked up automatically from the environment — never commit them to the repo.

**Linear** — authenticates via browser OAuth on first use (no token needed).

**Google Cloud** — requires the `gcloud` CLI to be installed and authenticated (`gcloud auth login`).

Restart your terminal and Claude Code. The token is picked up automatically from the environment — never commit it to the repo.

## Contributing

Please feel free to open issues or submit pull requests.

## Disclaimer

This project is experimental and under active development. Use at your own risk.
