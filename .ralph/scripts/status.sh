#!/bin/bash
# Show Ralph project status

CONFIG=".ralph/config.yaml"

if [[ ! -f "$CONFIG" ]]; then
  echo "Not a Ralph project (no .ralph/config.yaml found)"
  exit 1
fi

PROJECT=$(yq '.project.name // "Unknown"' "$CONFIG")
FEATURE_BRANCH=$(yq '.git.feature_branch // "ralph/feature"' "$CONFIG")

echo "=== Ralph Status ==="
echo ""
echo "Project: $PROJECT"
echo "Branch:  $(git branch --show-current 2>/dev/null || echo 'not in git repo')"
echo "Feature: $FEATURE_BRANCH"
echo ""

if [[ ! -f ".ralph/state/features.json" ]]; then
  echo "No features.json - run /ralph-plan first"
  exit 0
fi

TOTAL=$(jq '.features | length' .ralph/state/features.json)
PASSING=$(jq '[.features[] | select(.passes==true)] | length' .ralph/state/features.json)
FAILED=$(jq '[.features[] | select(.passes==false and .pr==null)] | length' .ralph/state/features.json)
IN_PR=$(jq '[.features[] | select(.pr!=null and .passes==false)] | length' .ralph/state/features.json)

echo "Features: $PASSING / $TOTAL complete"
echo "  - Passing:  $PASSING"
echo "  - Pending:  $FAILED"
echo "  - In PR:    $IN_PR"
echo ""

# Progress bar
if [[ "$TOTAL" -gt 0 ]]; then
  PCT=$((PASSING * 100 / TOTAL))
  FILLED=$((PCT / 5))
  EMPTY=$((20 - FILLED))
  BAR=$(printf '█%.0s' $(seq 1 $FILLED 2>/dev/null) || echo "")
  BAR+=$(printf '░%.0s' $(seq 1 $EMPTY 2>/dev/null) || echo "")
  echo "Progress: [$BAR] $PCT%"
  echo ""
fi

# Show next features
echo "Next up:"
jq -r '.features[] | select(.passes==false and .pr==null) | "  - \(.id): \(.description)"' .ralph/state/features.json | head -5

# Show features in PR
if [[ "$IN_PR" -gt 0 ]]; then
  echo ""
  echo "In PR (awaiting merge):"
  jq -r '.features[] | select(.pr!=null and .passes==false) | "  - \(.id) \(.pr): \(.description)"' .ralph/state/features.json
fi

# Show recent sessions
if [[ -f ".ralph/state/progress.txt" ]]; then
  echo ""
  echo "Recent sessions:"
  grep "^=== Session" .ralph/state/progress.txt | tail -3 | sed 's/^/  /'
fi
