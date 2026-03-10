#!/bin/bash
# Retry a specific feature or the last failed feature

set -e

FEATURE_ID="$1"
CONFIG=".ralph/config.yaml"

if [[ ! -f "$CONFIG" ]]; then
    echo "Not a Ralph project"
    exit 1
fi

if [[ ! -f ".ralph/state/features.json" ]]; then
    echo "No features.json found"
    exit 1
fi

# If no feature specified, find the first failed one
if [[ -z "$FEATURE_ID" ]]; then
    FEATURE_ID=$(jq -r '.features[] | select(.passes==false and .pr==null) | .id' .ralph/state/features.json | head -1)
    if [[ -z "$FEATURE_ID" ]]; then
        echo "No pending features to retry"
        exit 0
    fi
    echo "Retrying first pending feature: $FEATURE_ID"
fi

# Verify feature exists
EXISTS=$(jq -r ".features[] | select(.id==\"$FEATURE_ID\") | .id" .ralph/state/features.json)
if [[ -z "$EXISTS" ]]; then
    echo "Feature not found: $FEATURE_ID"
    echo ""
    echo "Available features:"
    jq -r '.features[].id' .ralph/state/features.json | head -10
    exit 1
fi

# Check if already passing
PASSES=$(jq -r ".features[] | select(.id==\"$FEATURE_ID\") | .passes" .ralph/state/features.json)
if [[ "$PASSES" == "true" ]]; then
    echo "Feature $FEATURE_ID already passes"
    read -p "Reset and retry? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        jq "(.features[] | select(.id==\"$FEATURE_ID\")).passes = false" \
            .ralph/state/features.json > /tmp/features.json \
            && mv /tmp/features.json .ralph/state/features.json
        echo "Reset $FEATURE_ID to pending"
    else
        exit 0
    fi
fi

# Clear any existing PR reference
jq "(.features[] | select(.id==\"$FEATURE_ID\")).pr = null" \
    .ralph/state/features.json > /tmp/features.json \
    && mv /tmp/features.json .ralph/state/features.json

DESCRIPTION=$(jq -r ".features[] | select(.id==\"$FEATURE_ID\") | .description" .ralph/state/features.json)
echo ""
echo "=== Retrying: $FEATURE_ID ==="
echo "Description: $DESCRIPTION"
echo ""

# Run the coding agent for this specific feature
RALPH_FEATURE="$FEATURE_ID" .ralph/scripts/run-agent.sh coding .ralph/prompts/coding-agent.md
