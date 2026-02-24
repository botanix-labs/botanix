# PRD: [Project Name]

> Use `/ralph-prd` command to generate this file interactively.

## Overview

[2-3 sentence description of what you're building]

## Goals

- [ ] Goal 1
- [ ] Goal 2
- [ ] Goal 3

## Tech Stack

- **Language**: [rust/typescript/python/go]
- **Framework**: [axum/express/fastapi/etc]
- **Database**: [postgres/mysql/sqlite/none]
- **External APIs**: [list any third-party services]

## Features

### Feature 1: [Name]

**Description:** [What it does]

**Acceptance Criteria:**

- [ ] Criterion 1
- [ ] Criterion 2
- [ ] Criterion 3

### Feature 2: [Name]

**Description:** [What it does]

**Acceptance Criteria:**

- [ ] Criterion 1
- [ ] Criterion 2

## Data Models

List the main entities/tables:

- **User**: id, email, name, created_at
- **Account**: id, user_id (FK), balance, status

## Constraints

- [Constraint 1: e.g., "Must integrate with existing auth system"]
- [Constraint 2: e.g., "Follow existing code patterns in src/"]

## Non-Goals (Out of Scope)

- [Thing we're NOT building in this phase]
- [Another thing to defer]

## Dependencies

- [External dependency 1]
- [Existing code to integrate with]

## Open Questions

- [ ] Question that needs answering before implementation
- [ ] Another open question
