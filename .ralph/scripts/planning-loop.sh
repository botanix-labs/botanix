#!/bin/bash
set -e

# Navigate to project root
cd "$(git rev-parse --show-toplevel 2> /dev/null || pwd)"

PRD_FILE="${1:-.ralph/PRD.md}"
MAX_PHASES=${2:-20}
CONFIG=".ralph/config.yaml"

# Get branch config
FEATURE_BRANCH=$(yq '.git.feature_branch // "ralph/feature"' "$CONFIG")
DEFAULT_BRANCH=$(yq '.git.default_branch // "main"' "$CONFIG")

echo "=== Ralph Planning Loop ==="
echo "PRD: $PRD_FILE"
echo "Feature branch: $FEATURE_BRANCH"
echo "Max phases: $MAX_PHASES"
echo ""

# Ensure we're on the feature branch
CURRENT_BRANCH=$(git branch --show-current)
if [[ "$CURRENT_BRANCH" != "$FEATURE_BRANCH" ]]; then
    echo "Switching to feature branch: $FEATURE_BRANCH"
    git checkout "$FEATURE_BRANCH" 2> /dev/null || {
        echo "ERROR: Feature branch '$FEATURE_BRANCH' not found."
        echo "Run: .ralph/scripts/init-feature-branch.sh"
        exit 1
    }
fi

# Check prerequisites
if [[ ! -f "$PRD_FILE" ]]; then
    echo "ERROR: PRD not found at $PRD_FILE"
    echo "Create one with /ralph-prd command."
    exit 1
fi

if [[ ! -f ".ralph/state/features.json" ]]; then
    echo "No features.json found. Running planner first..."
    .ralph/scripts/run-agent.sh planner .ralph/prompts/planner.md
fi

for PHASE in $(seq 1 $MAX_PHASES); do
    echo ""
    echo "╔════════════════════════════════════════════╗"
    echo "║              PHASE $PHASE                    ║"
    echo "╚════════════════════════════════════════════╝"
    echo ""

    # Get current status
    TOTAL=$(jq '.features | length' .ralph/state/features.json)
    PASSING=$(jq '[.features[] | select(.passes==true)] | length' .ralph/state/features.json)
    PENDING=$(jq '[.features[] | select(.passes==false and .pr==null)] | length' .ralph/state/features.json)
    IN_PR=$(jq '[.features[] | select(.pr!=null and .passes==false)] | length' .ralph/state/features.json)

    echo "Status: $PASSING/$TOTAL complete, $PENDING pending, $IN_PR in PR"

    # Check for completion
    if [[ "$PASSING" -eq "$TOTAL" && "$TOTAL" -gt 0 ]]; then
        echo ""
        echo "All features complete. Running planner to check for more work..."
        PLANNER_OUT=$(.ralph/scripts/run-agent.sh planner .ralph/prompts/planner.md 2>&1)

        if echo "$PLANNER_OUT" | grep -q "<planning>COMPLETE</planning>"; then
            echo ""
            echo "╔════════════════════════════════════════════╗"
            echo "║          PROJECT COMPLETE!                 ║"
            echo "╚════════════════════════════════════════════╝"
            echo ""
            echo "Total phases: $PHASE"
            echo "Total features: $TOTAL"
            exit 0
        fi

        echo "Planner added new features. Continuing..."
        echo ""
    fi

    # Count ready features
    SEQ_COUNT=$(jq '[.features[] | select(.passes==false and .parallel_safe==false and .pr==null)] | length' .ralph/state/features.json)
    PAR_COUNT=$(jq '[.features[] | select(.passes==false and .parallel_safe==true and .pr==null)] | length' .ralph/state/features.json)

    echo "Ready: $SEQ_COUNT sequential, $PAR_COUNT parallel"
    echo ""

    # Execute sequential features first (migrations, etc.)
    if [[ "$SEQ_COUNT" -gt 0 ]]; then
        echo ">>> Running sequential features..."
        .ralph/scripts/loop.sh 5 || true
        echo ""
    fi

    # Execute parallel features
    if [[ "$PAR_COUNT" -gt 0 ]]; then
        MAX_AGENTS=$(yq '.parallelization.max_agents // 3' .ralph/config.yaml)
        echo ">>> Running parallel features ($MAX_AGENTS agents)..."
        .ralph/scripts/parallel-loop.sh "$MAX_AGENTS" || true
        echo ""
    fi

    # Merge ready PRs
    echo ">>> Checking for PRs to merge..."
    MERGEABLE=$(gh pr list --state open --json number,mergeable -q '.[] | select(.mergeable=="MERGEABLE") | .number' 2> /dev/null || echo "")

    if [[ -n "$MERGEABLE" ]]; then
        echo "Merging PRs: $MERGEABLE"
        for PR in $MERGEABLE; do
            gh pr merge "$PR" --squash --delete-branch 2> /dev/null && echo "Merged PR #$PR" || echo "Failed to merge PR #$PR"
            sleep 1
        done

        # Pull merged changes to feature branch
        git pull origin "$FEATURE_BRANCH" 2> /dev/null || true

        # Update features.json for merged PRs
        for PR in $MERGEABLE; do
            FEATURE=$(jq -r ".features[] | select(.pr==\"#$PR\") | .id" .ralph/state/features.json)
            if [[ -n "$FEATURE" && "$FEATURE" != "null" ]]; then
                jq "(.features[] | select(.id==\"$FEATURE\")).passes = true" \
                    .ralph/state/features.json > /tmp/features.json \
                    && mv /tmp/features.json .ralph/state/features.json
                echo "Marked $FEATURE as passing"
            fi
        done
    else
        echo "No PRs ready to merge."
    fi

    # Run planner for next horizon
    if [[ "$PENDING" -eq 0 && "$IN_PR" -eq 0 ]]; then
        echo ""
        echo ">>> Running planner for next horizon..."
        .ralph/scripts/run-agent.sh planner .ralph/prompts/planner.md || true
    fi

    echo ""
    echo ">>> Phase $PHASE complete"
    echo ""
    echo "--- Next phase in 5 seconds (Ctrl+C to stop) ---"
    sleep 5
done

echo ""
echo "=== MAX PHASES REACHED ==="
PASSING=$(jq '[.features[] | select(.passes==true)] | length' .ralph/state/features.json)
TOTAL=$(jq '.features | length' .ralph/state/features.json)
echo "Completed $PASSING / $TOTAL features in $MAX_PHASES phases"
echo ""
echo "Check progress: cat .ralph/state/progress.txt"
echo "Check features: jq '.features[] | select(.passes==false)' .ralph/state/features.json"
