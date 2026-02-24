# Skill: Review Features

Review a features.json file for quality, completeness, and parallelization opportunities.

## When to Use

- After generating features with `/ralph-plan`
- User asks to review their features.json
- Before starting a long Ralph execution
- When debugging slow or failing executions

## Your Task

Analyze `.ralph/state/features.json` and provide actionable feedback.

## Review Process

### Step 1: Load Features

```bash
cat .ralph/state/features.json | jq '.meta'
cat .ralph/state/features.json | jq '.features | length'
cat .ralph/state/features.json | jq '[.features[] | .category] | group_by(.) | map({(.[0]): length}) | add'
```

### Step 2: Check Task Granularity

For each feature, verify:

- [ ] Is it ONE logical change?
- [ ] Can it complete in a single session?
- [ ] Does it have 3-8 steps (not too few, not too many)?

**Red flags:**

- Tasks with "and" in description (should be split)
- More than 8 steps
- Tasks touching many unrelated files

### Step 3: Check Step Quality

For each feature's steps:

- [ ] Are steps concrete and runnable?
- [ ] Do steps include verification?
- [ ] Are file paths explicit?
- [ ] Do steps reference actual commands?

**Good:**

```json
"steps": [
  "Create src/services/user.rs with UserService struct",
  "Add create_user() method that inserts into users table",
  "Run cargo check - verify compiles",
  "Add test in src/services/user_test.rs",
  "Run cargo test user_service - test passes"
]
```

**Bad:**

```json
"steps": [
  "Implement user service",
  "Test it"
]
```

### Step 4: Check Dependencies

Verify dependency graph:

- [ ] No circular dependencies
- [ ] Foundation features have no deps
- [ ] DB features depend on scaffold
- [ ] Model features depend on DB
- [ ] Service features depend on models
- [ ] Test features depend on implementation

### Step 5: Check Parallelization

Review `parallel_safe` assignments:

| Category   | Expected  | Verify                         |
| ---------- | --------- | ------------------------------ |
| scaffold   | true      | Unless creating shared imports |
| db         | **false** | Always sequential              |
| foundation | varies    | False if others import from it |
| model      | true      | After schema generated         |
| service    | true      | Different files                |
| handler    | true      | Different endpoints            |
| test       | true      | Independent tests              |
| final      | **false** | Always sequential              |

Look for:

- Features marked parallel that share files
- Features marked sequential that could be parallel
- Missing dependencies that would cause race conditions

### Step 6: Check Ordering

Features should be ordered by:

1. Scaffold (no deps)
2. Database (depends on scaffold)
3. Foundation (depends on db)
4. Models (depends on foundation)
5. Services (depends on models)
6. Handlers (depends on services)
7. Tests (depends on implementation)
8. Final (depends on all)

### Step 7: Estimate Execution

Calculate:

- Total features
- Sequential features (critical path)
- Parallel features
- Estimated speedup with N agents

```
Sequential time: [total] sessions
With 3 agents: [total - parallel + parallel/3] sessions
Speedup: [X]x
```

## Output Format

```markdown
## Features.json Review

### Summary

- Total features: X
- Categories: scaffold (N), db (N), model (N), ...
- Sequential: N features (critical path)
- Parallel: M features
- Estimated: ~Y sessions with 3 agents

### Strengths

- [What's good about this features.json]
- [Another positive]

### Issues Found

#### Critical (must fix)

- Feature X has circular dependency with Y
- Feature Z has vague steps

#### Warnings (should fix)

- Feature A could be split into smaller pieces
- Feature B is missing verification step

#### Suggestions (nice to have)

- Consider marking feature C as parallel_safe
- Add explicit file paths in feature D steps

### Parallelization Analysis

- Could be parallel but marked sequential: [list]
- Marked parallel but shares files: [list]
- Missing dependencies: [list]

### Recommended Changes

1. [Specific change with feature ID]
2. [Another specific change]
3. [...]

### Execution Plan

Phase 1 (sequential): scaffold-_, db-_
Phase 2 (parallel x3): model-_, foundation-_
Phase 3 (parallel x3): service-_
Phase 4 (parallel x3): handler-_, test-_
Phase 5 (sequential): final-_
```

## Common Issues

### 1. Too Many Dependencies

If most features have long `depends_on` lists, the graph may be over-constrained.

**Fix:** Only include direct dependencies, not transitive ones.

### 2. Too Few Dependencies

If parallel features don't have proper deps, they may race.

**Fix:** Add dependencies for any feature that imports from another.

### 3. Giant Features

Features with 10+ steps are hard to complete reliably.

**Fix:** Split into smaller features with explicit handoff.

### 4. Missing Verification

Steps without verification can't be marked as passing.

**Fix:** Add "Run X - verify Y" steps after each action.

### 5. Wrong Parallel Safety

Features touching shared files marked as parallel.

**Fix:** Mark as sequential OR split file ownership.
