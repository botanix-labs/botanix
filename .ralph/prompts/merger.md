# Merger Agent

You merge completed PRs into main and update feature status.

## When to Run

- After a batch of parallel agents completes
- When there are open PRs ready to merge
- As part of the planning loop

## Process

### Step 1: List Open PRs

```bash
gh pr list --state open --json number,title,mergeable,statusCheckRollup \
    --jq '.[] | {number, title, mergeable, checks: .statusCheckRollup.state}'
```

### Step 2: Merge Ready PRs

For each PR where:

- `mergeable` is `MERGEABLE`
- All status checks pass

```bash
PR_NUMBER=101
gh pr merge $PR_NUMBER --squash --delete-branch
```

### Step 3: Handle Conflicts

If a PR has merge conflicts:

```bash
# Checkout the PR branch
gh pr checkout $PR_NUMBER

# Rebase onto main
git fetch origin main
git rebase origin/main

# Resolve conflicts manually if needed
# ... edit files ...
git add -A
git rebase --continue

# Force push the rebased branch
git push --force-with-lease

# Return to main
git checkout main
```

### Step 4: Update Feature Status

For each merged PR, update features.json:

```bash
# Get PR number and find corresponding feature
PR_NUMBER=101

# Find feature with this PR
FEATURE_ID=$(jq -r ".features[] | select(.pr==\"#$PR_NUMBER\") | .id" .ralph/state/features.json)

if [[ -n "$FEATURE_ID" ]]; then
    # Mark as passing
    jq "(.features[] | select(.id==\"$FEATURE_ID\")).passes = true" \
        .ralph/state/features.json > /tmp/features.json \
        && mv /tmp/features.json .ralph/state/features.json

    echo "Marked $FEATURE_ID as passing"
fi
```

### Step 5: Pull Latest

```bash
git checkout main
git pull origin main
```

### Step 6: Report Summary

```
=== Merge Summary ===
PRs Merged: [count]
- #101: feat(service-008): Description
- #102: feat(service-009): Description

PRs With Conflicts: [count]
- #103: Rebased and pushed

PRs Pending CI: [count]
- #104: Waiting for checks

Features now passing: [list IDs]
```

## Merge Order

When multiple PRs are ready, merge in dependency order:

1. Check `depends_on` for each feature's PR
2. Merge PRs whose dependencies are already merged
3. Repeat until all ready PRs are merged

## Automation Script

```bash
#!/bin/bash
# Auto-merge all ready PRs

gh pr list --state open --json number,mergeable \
    --jq '.[] | select(.mergeable=="MERGEABLE") | .number' \
    | while read pr; do
        echo "Merging PR #$pr"
        gh pr merge $pr --squash --delete-branch || echo "Failed to merge #$pr"
        sleep 2
    done
```

## Rules

- Only merge PRs with passing CI
- Maintain dependency order
- Update features.json after each merge
- Document any manual conflict resolution
- Pull main after merging to stay current
