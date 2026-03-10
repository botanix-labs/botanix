#!/bin/bash
# Initialize the feature branch for Ralph work
# Run this once before starting Ralph execution

set -e

CONFIG=".ralph/config.yaml"

if [[ ! -f "$CONFIG" ]]; then
    echo "ERROR: Config not found at $CONFIG"
    exit 1
fi

DEFAULT_BRANCH=$(yq '.git.default_branch // "main"' "$CONFIG")
FEATURE_BRANCH=$(yq '.git.feature_branch // "ralph/feature"' "$CONFIG")

echo "=== Initializing Feature Branch ==="
echo "Default branch: $DEFAULT_BRANCH"
echo "Feature branch: $FEATURE_BRANCH"
echo ""

# Ensure we're on the default branch and up to date
git fetch origin "$DEFAULT_BRANCH"
git checkout "$DEFAULT_BRANCH"
git pull origin "$DEFAULT_BRANCH"

# Check if feature branch already exists
if git show-ref --verify --quiet "refs/heads/$FEATURE_BRANCH" 2> /dev/null; then
    echo "Feature branch '$FEATURE_BRANCH' already exists locally."
    read -p "Switch to it? (Y/n) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Nn]$ ]]; then
        git checkout "$FEATURE_BRANCH"
        git pull origin "$FEATURE_BRANCH" 2> /dev/null || echo "Branch not on remote yet."
    fi
elif git show-ref --verify --quiet "refs/remotes/origin/$FEATURE_BRANCH" 2> /dev/null; then
    echo "Feature branch '$FEATURE_BRANCH' exists on remote."
    git checkout -b "$FEATURE_BRANCH" "origin/$FEATURE_BRANCH"
else
    echo "Creating new feature branch '$FEATURE_BRANCH' from '$DEFAULT_BRANCH'..."
    git checkout -b "$FEATURE_BRANCH"
    git push -u origin "$FEATURE_BRANCH"
    echo "Feature branch created and pushed."
fi

echo ""
echo "=== Feature Branch Ready ==="
echo ""
echo "Current branch: $(git branch --show-current)"
echo ""
echo "All Ralph work will happen on: $FEATURE_BRANCH"
echo "PRs will merge into: $FEATURE_BRANCH"
echo "When complete, merge $FEATURE_BRANCH into $DEFAULT_BRANCH"
echo ""
echo "Next steps:"
echo "  1. Run .ralph/scripts/loop.sh (sequential)"
echo "  2. Or .ralph/scripts/parallel-loop.sh 3 (parallel)"
