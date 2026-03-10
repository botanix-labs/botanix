# Skill: Create PRD

Guide the user through creating a well-structured PRD (Product Requirements Document) that Ralph can execute.

## When to Use

- Starting a new project with Ralph
- Adding a major feature to an existing project
- User runs `/ralph-prd` command

## Your Task

Ask questions to understand what the user wants to build, then generate `.ralph/PRD.md`.

## Questions to Ask

### 1. What are you building?

- High-level description (1-2 sentences)
- Is this a new project or adding to existing code?
- What problem does it solve?

### 2. What's the tech stack?

- Language (Rust, TypeScript, Python, Go, etc.)
- Framework (Axum, Express, FastAPI, etc.)
- Database (Postgres, MySQL, SQLite, none)
- External APIs or services

### 3. What are the main features?

For each feature:

- What does it do?
- What does "done" look like?
- Are there specific requirements?

### 4. What are the data models?

- What entities/tables are needed?
- What are the relationships between them?
- Any specific fields required?

### 5. What are the constraints?

- Must integrate with existing code?
- Specific patterns to follow?
- Security requirements?
- Performance requirements?

### 6. What's the priority?

- What must be built first?
- What can be deferred to later phases?

### 7. What's out of scope?

- What are we NOT building?
- What can be deferred?

## Output Format

Generate `.ralph/PRD.md`:

```markdown
# PRD: [Project Name]

## Overview

[2-3 sentence description of what we're building and why]

## Goals

- [ ] Goal 1 - measurable outcome
- [ ] Goal 2 - measurable outcome
- [ ] Goal 3 - measurable outcome

## Tech Stack

- **Language**: [language]
- **Framework**: [framework]
- **Database**: [database]
- **External APIs**: [list or "None"]

## Features

### Feature 1: [Name]

**Description:** [What it does in 1-2 sentences]

**Acceptance Criteria:**

- [ ] Specific, testable criterion
- [ ] Another criterion
- [ ] Edge cases to handle

### Feature 2: [Name]

**Description:** [What it does]

**Acceptance Criteria:**

- [ ] Criterion 1
- [ ] Criterion 2

[Continue for all features...]

## Data Models

### [Entity Name]

- `id`: UUID, primary key
- `field_name`: type, constraints
- `created_at`: timestamp
- `updated_at`: timestamp

### [Another Entity]

- `id`: UUID, primary key
- `entity_id`: UUID, foreign key to Entity
- ...

## Constraints

- [Technical constraint]
- [Business constraint]
- [Integration requirement]

## Non-Goals (Out of Scope)

- [Thing we're explicitly NOT building]
- [Feature deferred to future phase]

## Dependencies

- [External service or API]
- [Existing code to integrate with]
- [Team or stakeholder dependency]

## Open Questions

- [ ] Question that needs answering
- [ ] Another open question
```

## PRD Quality Checklist

Before finishing, verify:

- [ ] Overview clearly explains what and why
- [ ] Goals are measurable
- [ ] Each feature has specific acceptance criteria
- [ ] Data models include all necessary fields
- [ ] Constraints are documented
- [ ] Scope boundaries are clear
- [ ] Dependencies are listed

## After Creating PRD

Tell the user:

```
PRD created at `.ralph/PRD.md`

Next steps:
1. Review the PRD and make any edits
2. Run `/ralph-plan` to generate executable features
3. Run `.ralph/scripts/setup.sh` to initialize Ralph
4. Run `.ralph/scripts/loop.sh` to start execution
```

## Tips for Good PRDs

1. **Be specific** - "User can log in" is vague. "User can log in with email/password, receives JWT token" is specific.

2. **Include edge cases** - What happens on error? Empty state? Invalid input?

3. **Define "done"** - Each feature should have criteria that prove it works.

4. **Scope ruthlessly** - It's better to build less completely than more partially.

5. **List assumptions** - What are you assuming about the environment, users, data?

## CRITICAL: Ask, Don't Assume

**If the user's answer is unclear, vague, or incomplete - ASK FOLLOW-UP QUESTIONS.**

**DO NOT:**

- Fill in blanks with assumptions
- Skip questions because you think you know the answer
- Create a PRD with placeholder values
- Proceed if critical information is missing

**Examples of when to probe deeper:**

| User says                  | Problem         | Ask                                                                          |
| -------------------------- | --------------- | ---------------------------------------------------------------------------- |
| "Standard auth"            | Ambiguous       | "Email/password? OAuth? SSO? What auth methods specifically?"                |
| "REST API"                 | Incomplete      | "What endpoints? What authentication? Who are the consumers?"                |
| "Use Postgres"             | Missing details | "Any specific version? Hosted or local? Do you have it set up already?"      |
| "It should be fast"        | Unmeasurable    | "What response time is acceptable? <100ms? <500ms?"                          |
| "Handle errors gracefully" | Vague           | "Show error messages? Retry? Log and alert? What's graceful for your users?" |

**Before generating the PRD, confirm:**

```
Before I create the PRD, let me confirm I have everything:

1. Core functionality: [summarize]
2. Tech stack: [summarize]
3. Data models: [summarize]
4. Key constraints: [summarize]

Is this correct? Anything I'm missing or misunderstanding?
```

A PRD with gaps will cause Ralph to either guess (wrong) or block (wasteful). Take the time to get it right.
