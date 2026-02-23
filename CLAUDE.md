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
