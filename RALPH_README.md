# Ralph: Parallel Agent Task Runner

Ralph breaks down PRDs into executable features and runs them with parallel agents.

## Features

- **Horizon Planning**: Plans in phases, adapts based on actual code
- **Parallel Execution**: Multiple agents work simultaneously on independent tasks
- **PR Workflow**: Each feature becomes a PR targeting a feature branch
- **Database Isolation**: Template DBs for resource-efficient parallel testing
- **CLI Tool**: Simple commands for common operations

## Quick Start

### Using the CLI (Recommended)

```bash
# Add ralph to your PATH (or copy the ralph script to /usr/local/bin)
export PATH="/path/to/ralph:$PATH"

# Initialize Ralph in your project
cd your-project
ralph init my-project

# Create PRD and generate features
ralph prd  # Interactive PRD creation
ralph plan # Generate features from PRD

# Initialize feature branch
ralph branch

# Run Ralph
ralph run        # Sequential
ralph parallel 3 # Parallel with 3 agents
ralph auto       # Autonomous planning loop

# Check progress
ralph status

# When complete
ralph finalize
```

### CLI Commands

```
ralph init [name]       Initialize Ralph in current directory
ralph status            Show project status and progress
ralph plan              Generate features from PRD
ralph run [n]           Run sequential loop (n iterations)
ralph parallel [n]      Run with n parallel agents (default 3)
ralph auto [n]          Run autonomous planning loop
ralph retry [id]        Retry a specific feature
ralph branch            Initialize/switch to feature branch
ralph finalize          Merge feature branch to main
ralph reset [id]        Reset a feature to pending
ralph clean             Clean logs and reset state
ralph help              Show help
```

### Manual Setup (Alternative)

```bash
# Copy Ralph to your project
cp -r ralph/.ralph your-repo/

# Configure
cd your-repo
vim .ralph/config.yaml

# Run scripts directly
ralph prd    # Create PRD
ralph plan   # Generate features
ralph branch # Init feature branch
ralph run    # Run
```

## Prerequisites

```bash
# Required
brew install jq yq gh

# Optional - for GitHub PR workflow
# gh auth login

# Optional - PostgreSQL for DB isolation (only if your project uses Postgres)
# brew install postgresql

# Optional - GNU parallel for better parallel execution (bash fallback works fine)
# brew install parallel
```

**Required:**

- `jq` - JSON parsing for features.json
- `yq` - YAML parsing for config.yaml
- `gh` - GitHub CLI for PR creation
- An AI CLI tool (claude, droid, openai-cli, etc.)

**Optional:**

- `postgresql` - Only if your project needs isolated DBs per agent
- `parallel` - Slightly better parallel execution; bash backgrounding works without it

## Configuration

Edit `.ralph/config.yaml`:

```yaml
project:
    name: my-project
    language: rust # rust, typescript, python, go

# Different models for different agent types
models:
    planner:
        provider: "claude"
        model: "claude-opus-4" # Best for architecture/planning
    coding:
        provider: "claude"
        model: "claude-sonnet-4" # Good balance for coding
    merger:
        provider: "openai"
        model: "gpt-4o" # Or use claude
    default:
        provider: "claude"
        model: "claude-sonnet-4"

# CLI commands per provider
cli:
    claude: "claude --print --model {model}"
    openai: "openai-cli chat --model {model}"
    droid: "droid exec -m {model} --auto high -f {prompt_file}"

parallelization:
    max_agents: 3
    pr_workflow: true

validators:
    check: "cargo check"
    lint: "cargo clippy --fix --allow-dirty"
    format: "cargo fmt --all"
    test: "cargo test --lib"
```

### Model Selection Tips

| Agent   | Recommended           | Why                                     |
| ------- | --------------------- | --------------------------------------- |
| Planner | claude-opus-4, gpt-4o | Needs strong reasoning for architecture |
| Coding  | claude-sonnet-4       | Good balance of speed and quality       |
| Merger  | claude-sonnet-4       | Git operations, conflict resolution     |

For cost optimization, use cheaper models for simpler tasks:

```yaml
models:
    planner:
        model: "claude-opus-4" # Expensive but worth it for planning
    coding:
        model: "claude-sonnet-4" # Mid-tier for implementation
```

## Example Workflows

### Basic Flow: New Feature from Scratch

```bash
# 1. Initialize Ralph in your project
cd my-rust-api
ralph init my-api

# 2. Create your PRD interactively
ralph prd
# → Agent asks questions about what you're building
# → Generates .ralph/PRD.md with structured requirements
# → Prompts: "Generate features from PRD now? (Y/n)"
# → If yes, spawns a NEW agent session for planning

# 3. (Optional) Review generated features
ralph status
cat .ralph/state/features.json

# 4. Create feature branch for all Ralph work
ralph branch
# → Creates ralph/my-feature branch from main
# → All PRs will target this branch, not main

# 5. Execute features
ralph parallel 3
# → Runs up to 3 agents simultaneously
# → Each agent works on independent features
# → Creates PRs that merge to feature branch

# 6. When complete, merge to main
ralph finalize
# → Squash merges feature branch to main
# → Cleans up feature branch
```

### Using Different AI Providers

```bash
# Use Claude Opus for planning (better reasoning)
ralph --provider claude --model claude-opus-4 prd

# Use Droid with GPT-5.2 Codex for coding
ralph --provider droid --model gpt-5.2-codex parallel 3

# Mix providers in config for different agent types
# Edit .ralph/config.yaml:
#   planner: claude-opus-4 (architecture decisions)
#   coding: gpt-5.2-codex (implementation)
#   merger: claude-sonnet-4 (git operations)
```

### Autonomous Mode: Hands-Off Execution

```bash
# Run full autonomous loop (plan → execute → replan → repeat)
ralph auto 10
# → Runs up to 10 planning phases
# → Each phase: analyze progress, plan next features, execute
# → Stops when PRD is satisfied or max phases reached
```

### Recovering from Failures

```bash
# Check what failed
ralph status

# Retry a specific feature
ralph retry service-003

# Reset a feature to pending (re-run from scratch)
ralph reset handler-005

# Nuclear option: reset everything
ralph clean
```

## How Parallel Agents Work

### The Problem with Sequential Execution

Traditional AI coding runs one task at a time. For a 50-feature project at 2 minutes per feature, that's ~100 minutes of wall-clock time.

### Ralph's Parallel Solution

Ralph analyzes feature dependencies and runs independent features simultaneously:

```
Sequential (100 min):
  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐
  │ F1  │→│ F2  │→│ F3  │→│ F4  │→│ ... │
  └─────┘ └─────┘ └─────┘ └─────┘ └─────┘

Parallel with 3 agents (~35 min):
  Agent 1: ┌─────┐ ┌─────┐ ┌─────┐
           │ F1  │→│ F4  │→│ F7  │→ ...
           └─────┘ └─────┘ └─────┘
  Agent 2: ┌─────┐ ┌─────┐ ┌─────┐
           │ F2  │→│ F5  │→│ F8  │→ ...
           └─────┘ └─────┘ └─────┘
  Agent 3: ┌─────┐ ┌─────┐ ┌─────┐
           │ F3  │→│ F6  │→│ F9  │→ ...
           └─────┘ └─────┘ └─────┘
```

### How It Decides What's Parallel-Safe

The planner marks each feature with `parallel_safe: true/false`:

| Feature Type   | Parallel Safe? | Reason                                      |
| -------------- | -------------- | ------------------------------------------- |
| `scaffold-*`   | Usually yes    | Directory structure, no conflicts           |
| `db-*`         | **No**         | Migrations must run in order                |
| `foundation-*` | Depends        | False if other features import from it      |
| `model-*`      | Yes            | After schema exists, models are independent |
| `service-*`    | Yes            | Different files, no cross-imports           |
| `handler-*`    | Yes            | Independent API endpoints                   |
| `test-*`       | Yes            | Tests don't modify shared state             |
| `final-*`      | **No**         | Cleanup/formatting needs all code present   |

### Execution Order

```
Phase 1 (sequential):  scaffold-001 → db-001 → db-002 → foundation-001
Phase 2 (parallel x3): model-001, model-002, model-003
Phase 3 (parallel x3): service-001, service-002, service-003
Phase 4 (parallel x3): handler-001, handler-002, test-001
Phase 5 (sequential):  final-001 (format, lint, final tests)
```

### Isolation: How Agents Don't Conflict

Each parallel agent gets:

1. **Own Git Branch**: `pr/feature-001`, `pr/feature-002`, etc.
2. **Own Database** (optional): If configured, Ralph creates per-agent DB copies
3. **Shared Build Cache**: Cargo/npm target directories are shared (read-heavy)

**Database isolation** (only if needed - most projects can skip this):

```yaml
# In .ralph/config.yaml - only for projects with Postgres that need test isolation
database:
    from_env: "DATABASE_URL" # Reads from your .env file


# WARNING: Only use LOCAL/DEV databases! Never production.
```

```
     ┌─────┴─────┐  ┌─────┴─────┐  ┌─────┴─────┐
     │  Agent 1  │  │  Agent 2  │  │  Agent 3  │
     │ Branch:   │  │ Branch:   │  │ Branch:   │
     │ pr/svc-01 │  │ pr/svc-02 │  │ pr/mdl-03 │
     │ DB: ag_01 │  │ DB: ag_02 │  │ DB: ag_03 │
     └─────┬─────┘  └─────┬─────┘  └─────┬─────┘
           │              │              │
           ▼              ▼              ▼
        PR #101        PR #102        PR #103
           │              │              │
           └──────────────┼──────────────┘
                          ▼
                 ralph/feature branch
                          │
                          ▼ (ralph finalize)
                        main
```

### PR Workflow

Each completed feature becomes a PR:

- **Source**: Agent's working branch (`pr/service-001`)
- **Target**: Feature branch (`ralph/my-feature`), NOT main
- **Validation**: Must pass check, lint, format, test before PR
- **Merge**: Auto-merged to feature branch on success

This keeps `main` clean until the entire feature set is complete.

## Directory Structure

```
.ralph/
├── PRD.md                 # Your requirements
├── config.yaml            # Project configuration
├── prompts/
│   ├── planner.md         # Horizon planning agent
│   ├── coding-agent.md    # Worker agent
│   └── merger.md          # PR merge agent
├── skills/
│   ├── create-prd.md      # PRD creation guide
│   ├── prd-to-features.md # Feature generation
│   └── review-features.md # Feature review
├── scripts/
│   ├── setup.sh           # One-time setup
│   ├── loop.sh            # Sequential runner
│   ├── parallel-loop.sh   # Parallel runner
│   ├── planning-loop.sh   # Full autonomous loop
│   └── agent-db.sh        # DB isolation helper
├── state/
│   ├── features.json      # Generated features
│   └── progress.txt       # Session logs
└── logs/
    └── *.log              # Per-session logs
```

## Resource Usage

| Component    | Count | RAM                       |
| ------------ | ----- | ------------------------- |
| PostgreSQL   | 1     | ~200MB                    |
| Agent DBs    | N     | ~0MB each (copy-on-write) |
| Cargo target | 1     | Shared                    |

Run 10-20 parallel agents on a 16GB Mac.

## License

MIT
