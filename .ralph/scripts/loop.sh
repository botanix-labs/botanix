#!/bin/bash
set -e

# Navigate to project root
cd "$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

MAX_ITERATIONS=${1:-50}
SESSION=1

echo "=== Ralph Sequential Loop ==="
echo "Max iterations: $MAX_ITERATIONS"
echo ""

# Check for features
if [[ ! -f ".ralph/state/features.json" ]]; then
  echo "ERROR: No features.json found."
  echo "Run /ralph-plan to generate features first."
  exit 1
fi

TOTAL=$(jq '.features | length' .ralph/state/features.json)
if [[ "$TOTAL" -eq 0 ]]; then
  echo "ERROR: features.json is empty."
  echo "Run /ralph-plan to generate features first."
  exit 1
fi

echo "Total features: $TOTAL"
echo ""

TIMESTAMP=$(date +%Y%m%d-%H%M%S)

while [ $SESSION -le $MAX_ITERATIONS ]; do
  LOG_FILE=".ralph/logs/${TIMESTAMP}-session-$(printf '%03d' $SESSION).log"

  # Get next incomplete feature
  FEATURE=$(jq -r '.features[] | select(.passes==false) | .id' .ralph/state/features.json | head -1)

  if [[ -z "$FEATURE" ]]; then
    PASSING=$(jq '[.features[] | select(.passes==true)] | length' .ralph/state/features.json)
    echo ""
    echo "=== ALL FEATURES COMPLETE ==="
    echo "Total sessions: $((SESSION - 1))"
    echo "Features completed: $PASSING / $TOTAL"
    exit 0
  fi

  DESCRIPTION=$(jq -r ".features[] | select(.id==\"$FEATURE\") | .description" .ralph/state/features.json)
  PASSING=$(jq '[.features[] | select(.passes==true)] | length' .ralph/state/features.json)

  echo "=== Session $SESSION | $FEATURE ===" | tee "$LOG_FILE"
  echo "Progress: $PASSING / $TOTAL complete" | tee -a "$LOG_FILE"
  echo "Description: $DESCRIPTION" | tee -a "$LOG_FILE"
  echo "" | tee -a "$LOG_FILE"

  # Run coding agent (uses model from config.yaml)
  RALPH_FEATURE="$FEATURE" .ralph/scripts/run-agent.sh coding .ralph/prompts/coding-agent.md 2>&1 | tee -a "$LOG_FILE"

  RESULT=$?
  echo "" | tee -a "$LOG_FILE"
  echo "=== Session $SESSION complete (exit: $RESULT) ===" | tee -a "$LOG_FILE"

  SESSION=$((SESSION + 1))

  # Brief pause between sessions
  if [ $SESSION -le $MAX_ITERATIONS ]; then
    echo ""
    echo "--- Next session in 3 seconds (Ctrl+C to stop) ---"
    sleep 3
  fi
done

echo ""
echo "=== MAX ITERATIONS REACHED ==="
PASSING=$(jq '[.features[] | select(.passes==true)] | length' .ralph/state/features.json)
echo "Completed $PASSING / $TOTAL features in $MAX_ITERATIONS sessions"
echo "Run again to continue."
