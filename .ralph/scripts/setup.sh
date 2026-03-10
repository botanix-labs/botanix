#!/bin/bash
set -e

echo "=== Ralph Setup ==="

# Check dependencies
echo "Checking dependencies..."

MISSING=""
OPTIONAL_MISSING=""
command -v jq >/dev/null || MISSING="$MISSING jq"
command -v yq >/dev/null || MISSING="$MISSING yq"
command -v gh >/dev/null || MISSING="$MISSING gh"
command -v parallel >/dev/null || OPTIONAL_MISSING="$OPTIONAL_MISSING parallel"

if [[ -n "$MISSING" ]]; then
  echo "Missing required dependencies:$MISSING"
  echo ""
  echo "Install with:"
  echo "  brew install$MISSING"
  exit 1
fi

echo "Required dependencies found."

if [[ -n "$OPTIONAL_MISSING" ]]; then
  echo "Optional (not required):$OPTIONAL_MISSING"
fi

# Load config
CONFIG=".ralph/config.yaml"
if [[ ! -f "$CONFIG" ]]; then
  echo ""
  echo "ERROR: Config not found at $CONFIG"
  echo ""
  echo "Create one by copying the template:"
  echo "  cp .ralph/config.yaml.template .ralph/config.yaml"
  echo ""
  echo "Then edit it for your project."
  exit 1
fi

PROJECT_NAME=$(yq '.project.name' "$CONFIG")
echo ""
echo "Project: $PROJECT_NAME"

# Database check
DB_FROM_ENV=$(yq '.database.from_env // ""' "$CONFIG" 2>/dev/null)
DB_URL=$(yq '.database.url // ""' "$CONFIG" 2>/dev/null)

if [[ -n "$DB_FROM_ENV" && "$DB_FROM_ENV" != "null" ]]; then
  echo ""
  echo "Database isolation: enabled (from_env: $DB_FROM_ENV)"
  echo "Ralph will create per-agent DB copies for parallel isolation."
  echo "WARNING: Ensure this points to a LOCAL/DEV database only!"
elif [[ -n "$DB_URL" && "$DB_URL" != "null" ]]; then
  echo ""
  echo "Database isolation: enabled (direct URL)"
  echo "WARNING: Ensure this is a LOCAL/DEV database only!"
fi

# Create directories
echo ""
echo "Creating directories..."
mkdir -p .ralph/logs .ralph/state

# Initialize state files
if [[ ! -f ".ralph/state/features.json" ]]; then
  echo '{"meta":{},"features":[]}' > .ralph/state/features.json
  echo "Created empty features.json"
fi

if [[ ! -f ".ralph/state/progress.txt" ]]; then
  cat > .ralph/state/progress.txt << EOF
=== Ralph Progress Log ===
Project: $PROJECT_NAME
Initialized: $(date -Iseconds)

--- Sessions Below ---
EOF
  echo "Created progress.txt"
fi

# Make scripts executable
chmod +x .ralph/scripts/*.sh 2>/dev/null || true

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Next steps:"
echo "  1. Create your PRD:      Use /ralph-prd command"
echo "  2. Generate features:    Use /ralph-plan command"
echo "  3. Run Ralph:            .ralph/scripts/loop.sh"
echo ""
echo "For parallel execution:    .ralph/scripts/parallel-loop.sh 3"
echo "For autonomous planning:   .ralph/scripts/planning-loop.sh"
