# Skill: PRD to Features

Transform a PRD into executable `features.json` with proper dependencies and parallelization analysis.

## When to Use

- After creating a PRD with `/ralph-prd`
- User runs `/ralph-plan` command
- Re-planning after major PRD changes

## Prerequisites

- `.ralph/PRD.md` must exist
- `.ralph/config.yaml` should be configured

## Your Task

### Step 1: Read Inputs

```bash
cat .ralph/PRD.md
cat .ralph/config.yaml
```

### Step 2: Analyze PRD

Extract and categorize:

1. **Entities** - Data models, tables, types needed
2. **Features** - Distinct pieces of functionality
3. **Dependencies** - What needs what (FK relationships, imports)
4. **Patterns** - Architectural patterns required

### Step 3: Design Architecture

Based on PRD, identify needed patterns:

| Pattern         | Trigger                    | Files to Create                                |
| --------------- | -------------------------- | ---------------------------------------------- |
| Provider trait  | Multiple external services | `providers/traits.rs`, `providers/registry.rs` |
| Shared crypto   | Encryption, PII            | `common/crypto.rs`                             |
| Error handling  | Any project                | `common/error.rs`                              |
| Database models | Has database               | `models/*.rs`                                  |

### Step 4: Generate Features

Create features in this order:

#### Phase 1: Scaffold (parallel_safe: true)

- Directory structure
- Empty modules with declarations
- Dependencies in Cargo.toml/package.json

#### Phase 2: Database (parallel_safe: false)

- Single migration with all tables OR
- Ordered migrations respecting FK dependencies
- Schema generation (diesel print-schema, prisma generate)

#### Phase 3: Foundation (parallel_safe: varies)

- Shared error types
- Common traits/interfaces
- Utility functions

#### Phase 4: Implementation (parallel_safe: true)

- Models (after schema)
- Services (after models)
- Handlers (after services)

#### Phase 5: Tests (parallel_safe: true)

- Unit tests
- Integration tests

#### Phase 6: Final (parallel_safe: false)

- Format entire codebase
- Lint fixes
- Final validation

### Step 5: Write features.json

```json
{
    "meta": {
        "prd_source": ".ralph/PRD.md",
        "generated_at": "2024-01-22T12:00:00Z",
        "total_features": 45,
        "parallel_features": 32,
        "sequential_features": 13,
        "estimated_phases": 6
    },
    "features": [
        {
            "id": "scaffold-001",
            "category": "scaffold",
            "description": "Create project directory structure",
            "depends_on": [],
            "parallel_safe": true,
            "pr": null,
            "passes": false,
            "steps": [
                "mkdir -p src/{models,services,handlers,common}",
                "Create src/lib.rs with module declarations",
                "Create src/main.rs with basic setup",
                "Run cargo check - verify compiles"
            ]
        },
        {
            "id": "db-001",
            "category": "database",
            "description": "Create database migration with all tables",
            "depends_on": ["scaffold-001"],
            "parallel_safe": false,
            "pr": null,
            "passes": false,
            "steps": [
                "Run diesel migration generate create_tables",
                "Add CREATE TABLE statements for all entities",
                "Add indexes and constraints",
                "Run diesel migration run",
                "Run diesel migration redo - verify rollback works",
                "Run diesel print-schema > src/schema.rs"
            ]
        }
    ]
}
```

### Step 6: Initialize Progress

Create `.ralph/state/progress.txt`:

```
=== Ralph Progress Log ===
Project: [from config.yaml]
PRD: .ralph/PRD.md
Generated: [timestamp]
Total Features: [count]

--- Planning Notes ---
Architecture decisions:
- [Decision 1 and rationale]
- [Decision 2 and rationale]

Parallelization strategy:
- Sequential: scaffold (1), db (N), final (M)
- Parallel: models (X), services (Y), tests (Z)

--- Sessions Below ---
```

### Step 7: Output Summary

```
=== Features Generated ===

Total: 45 features
- scaffold: 3 (parallel)
- database: 5 (sequential)
- foundation: 4 (mixed)
- model: 10 (parallel)
- service: 12 (parallel)
- handler: 8 (parallel)
- test: 10 (parallel)
- final: 3 (sequential)

Estimated execution time:
- Sequential only: ~45 sessions
- With 3 agents: ~20 sessions (2.25x speedup)

Critical path: scaffold → db → foundation → [parallel work] → final

Next steps:
1. Review .ralph/state/features.json
2. Run .ralph/scripts/setup.sh
3. Run .ralph/scripts/loop.sh (sequential) OR
4. Run .ralph/scripts/parallel-loop.sh 3 (parallel)
```

## Feature Writing Rules

### ID Format

```
{category}-{NNN}

scaffold-001, scaffold-002
db-001, db-002
foundation-001
model-001, model-002
service-001, service-002
handler-001 or api-001
test-001, test-002
final-001
```

### Step Quality

Every step must be:

- **Binary** - Pass or fail, no "mostly works"
- **Specific** - Exact command, exact file, exact verification
- **Executable** - Agent can run it without interpretation

**Good steps:**

```json
"steps": [
  "Create src/models/user.rs with User struct",
  "Add #[derive(Queryable, Selectable)] to User",
  "Add pub mod user; to src/models/mod.rs",
  "Run cargo check - verify compiles"
]
```

**Bad steps:**

```json
"steps": [
  "Create the user model",
  "Make sure it works",
  "Add tests"
]
```

### Dependencies

- Use actual feature IDs in `depends_on`
- No circular dependencies
- Foundation features typically have no deps
- Everything else deps on relevant foundation

### Parallelization Rules

```
parallel_safe: true when:
  ✓ Different files with no imports between them
  ✓ Independent database tables (after schema exists)
  ✓ Tests for different modules
  ✓ Separate API endpoints

parallel_safe: false when:
  ✗ Database migrations
  ✗ Shared trait/type definitions
  ✗ mod.rs files that others import from
  ✗ Final formatting/linting
  ✗ Anything that must happen before parallel work
```

## Anti-Patterns to Avoid

1. **Monster features** - If >8 steps, split it
2. **Vague steps** - "Make it work" is not a step
3. **Missing verification** - Every action needs a check
4. **Assumed context** - Each feature should be self-contained
5. **Skipping error cases** - Include failure scenarios
6. **Wrong dependencies** - Don't create false deps that prevent parallelization
