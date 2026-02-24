# Claude Code Configuration

@PROJECT.md

## Getting Started with Claude Code

### Initial Setup

```bash
./scripts/setup.sh         # Install pre-commit hooks, bun, cargo tools
./scripts/update-agents.sh # Install AI agent skills from botanix-labs/botanix-skills
```

To install skills for Claude Code only:

```bash
./scripts/update-agents.sh claude-code
```

### Available Slash Commands

- `/review-pr` — comprehensive PR review using specialized agents
- `/commit` — create a commit from staged/unstaged changes

### Key Make Targets

```bash
make build       # Release build
make build-debug # Debug build
make test-unit   # Run unit tests (nextest)
make fmt         # Format everything (Rust + TOML + Prettier + Markdown)
make lint        # Run all linters (clippy, fmt check, taplo, prettier, machete)
make lint-clippy # Clippy only
make docs        # Generate documentation
make audit       # Cargo audit for vulnerabilities
```

### MCP Servers

Configured in `.mcp.json` (auto-enabled via `.claude/settings.json`):

- **GitHub** — issues, PRs, project boards (`GITHUB_PERSONAL_ACCESS_TOKEN` env var required)
- **Linear** — issue tracking
- **Google Cloud** — general GCP interaction via gcloud CLI (requires `gcloud` CLI authenticated)
- **Google Cloud Storage** — GCS bucket and object operations
- **Google Cloud Observability** — logs, metrics, traces, error reports

### Local Dev Environment

```bash
make init-docker-local  # Initialize local docker setup
make start-docker-local # Start local network
make stop-docker-local  # Stop local network
```

## Git

When creating git commits, do not include the Co-Authored-By: Claude trailer.

## Workflow

When reviewing branch changes, run these checks (in parallel where possible):

1. Check for silent failures
2. Verify code comments are accurate
3. Review any new types
4. General code review
5. `make fmt` (fix any issues)
6. `make lint`
7. `make test-unit`
8. Write a short summary of important changes for PR description

## Stack

- Language: Rust
- Formatter: `cargo fmt`
- Linter: `cargo clippy`
- Tests: `cargo test`

## Workflow

### Review Checklist

When asked to review changes in the branch, run these checks in parallel:

- Check for silent failures (unwrap without context, swallowed errors)
- Verify code comments are accurate
- Review any new types for correctness and naming
- General code review (logic, edge cases, performance)
- Run `cargo fmt --check` (fix any issues)
- Run `cargo clippy` and address warnings
- Write a short summary of all important changes for PR description

### Before Marking Done

- `cargo fmt` passes
- `cargo clippy` has no warnings
- `cargo test` passes
- No `.unwrap()` on user-facing paths — use proper error handling

## Rust Conventions

- Prefer `?` operator over `.unwrap()` for error propagation
- Use `thiserror` for custom error types, `anyhow` for application errors
- Derive `Debug` on all public types
- Keep functions small and focused — extract when over ~40 lines
- Prefer strong types over primitive obsession (newtype pattern for IDs, amounts)

## DeFi-Specific (This Repo)

- All token amounts must use the correct decimals from config — never hardcode
- Contract addresses come from verified address lists, never inline strings
- Any chain interaction must handle revert cases explicitly
- Log all on-chain call parameters at debug level for troubleshooting
