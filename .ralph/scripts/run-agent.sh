#!/bin/bash
# Run an agent with the configured model/CLI
# Usage: ./run-agent.sh <agent_type> <prompt_file> [extra_args...]
#
# Agent types: planner, coding, merger, ideate, prd, reviewer, default
#
# Environment variables:
#   RALPH_MODEL_OVERRIDE    - Override model from CLI
#   RALPH_PROVIDER_OVERRIDE - Override provider from CLI
#   RALPH_MODEL             - Same as above (legacy)
#   RALPH_PROVIDER          - Same as above (legacy)
#
# Examples:
#   ./run-agent.sh coding .ralph/prompts/coding-agent.md
#   ./run-agent.sh planner .ralph/prompts/planner.md
#   RALPH_MODEL_OVERRIDE=claude-opus-4 ./run-agent.sh planner .ralph/prompts/planner.md

set -e

AGENT_TYPE="${1:-coding}"
PROMPT_FILE="${2:-.ralph/prompts/coding-agent.md}"
shift 2 2>/dev/null || true

CONFIG=".ralph/config.yaml"

# Load prompt helpers
if [[ -f ".ralph/scripts/prompt-user.sh" ]]; then
  source .ralph/scripts/prompt-user.sh
fi

if [[ ! -f "$CONFIG" ]]; then
  echo "ERROR: Config not found at $CONFIG" >&2
  exit 1
fi

if [[ ! -f "$PROMPT_FILE" ]]; then
  echo "ERROR: Prompt file not found: $PROMPT_FILE" >&2
  exit 1
fi

# Check for overrides from environment (CLI flags)
MODEL_OVERRIDE="${RALPH_MODEL_OVERRIDE:-$RALPH_MODEL}"
PROVIDER_OVERRIDE="${RALPH_PROVIDER_OVERRIDE:-$RALPH_PROVIDER}"

# Get model config for this agent type (fall back to default)
if [[ -n "$PROVIDER_OVERRIDE" ]]; then
  PROVIDER="$PROVIDER_OVERRIDE"
else
  PROVIDER=$(yq ".models.${AGENT_TYPE}.provider // .models.default.provider // \"claude\"" "$CONFIG")
fi

if [[ -n "$MODEL_OVERRIDE" ]]; then
  MODEL="$MODEL_OVERRIDE"
else
  MODEL=$(yq ".models.${AGENT_TYPE}.model // .models.default.model // \"claude-sonnet-4\"" "$CONFIG")
fi

# Check for custom command
if [[ "$PROVIDER" == "custom" ]]; then
  CUSTOM_CMD=$(yq ".models.${AGENT_TYPE}.command" "$CONFIG")
  if [[ -n "$CUSTOM_CMD" && "$CUSTOM_CMD" != "null" ]]; then
    CMD="$CUSTOM_CMD"
  else
    echo "ERROR: Custom provider requires 'command' to be set" >&2
    exit 1
  fi
else
  # Get CLI template for this provider
  CLI_TEMPLATE=$(yq ".cli.${PROVIDER} // \"claude --print --model {model}\"" "$CONFIG")
  # Replace placeholders
  CMD="${CLI_TEMPLATE//\{model\}/$MODEL}"
  CMD="${CMD//\{prompt_file\}/$PROMPT_FILE}"
fi

# Extract the base command (first word) to check if it exists
BASE_CMD=$(echo "$CMD" | awk '{print $1}')

if ! command -v "$BASE_CMD" >/dev/null 2>&1; then
  echo "" >&2
  echo "ERROR: Command not found: $BASE_CMD" >&2
  echo "" >&2
  echo "Provider '$PROVIDER' requires '$BASE_CMD' to be installed." >&2
  echo "" >&2

  if type ralph_require &>/dev/null; then
    case "$BASE_CMD" in
      claude)
        ralph_require "$BASE_CMD" "Install: npm install -g @anthropic-ai/claude-cli"
        ;;
      droid)
        ralph_require "$BASE_CMD" "Install: npm install -g @anthropic-ai/droid-cli (or visit https://docs.factory.ai)"
        ;;
      *)
        ralph_require "$BASE_CMD" "Install the CLI tool or update .ralph/config.yaml with correct provider"
        ;;
    esac
  else
    exit 1
  fi
fi

# Log what we're running
echo "=== Running $AGENT_TYPE agent ===" >&2
echo "Provider: $PROVIDER" >&2
echo "Model: $MODEL" >&2
echo "Command: $CMD" >&2
echo "Prompt: $PROMPT_FILE" >&2
echo "" >&2

# Check if command uses {prompt_file} (file-based input)
if [[ "$CLI_TEMPLATE" == *"{prompt_file}"* ]]; then
  # Command handles file directly (e.g., droid exec -f)
  eval "$CMD" "$@"
else
  # Pipe prompt content to command (e.g., claude)
  cat "$PROMPT_FILE" | eval "$CMD" "$@"
fi
