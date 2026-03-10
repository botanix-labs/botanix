#!/bin/bash
# Helper for user-in-the-loop prompts
# Usage: source prompt-user.sh
#
# Functions:
#   ralph_prompt "message" [default]  - Prompt user, return input or default
#   ralph_confirm "message"           - Yes/no prompt, returns 0 for yes
#   ralph_require "what" "hint"       - Fail/prompt for required thing
#   ralph_warn "message"              - Warning that can be skipped or halt

CONFIG="${CONFIG:-.ralph/config.yaml}"

# Check if interactive mode
is_interactive() {
    # Non-interactive if: CI=true, or config says interactive: false
    if [[ "$CI" == "true" || "$RALPH_NON_INTERACTIVE" == "true" ]]; then
        return 1
    fi

    if [[ -f "$CONFIG" ]]; then
        local INTERACTIVE=$(yq '.interactive // true' "$CONFIG" 2> /dev/null)
        [[ "$INTERACTIVE" == "true" ]]
        return $?
    fi

    # Default to interactive
    return 0
}

# Prompt for input
# Usage: VALUE=$(ralph_prompt "Enter value" "default")
ralph_prompt() {
    local MESSAGE="$1"
    local DEFAULT="$2"

    if is_interactive; then
        if [[ -n "$DEFAULT" ]]; then
            read -p "$MESSAGE [$DEFAULT]: " VALUE
            echo "${VALUE:-$DEFAULT}"
        else
            read -p "$MESSAGE: " VALUE
            echo "$VALUE"
        fi
    else
        echo "$DEFAULT"
    fi
}

# Yes/no confirmation
# Usage: if ralph_confirm "Continue?"; then ...
ralph_confirm() {
    local MESSAGE="$1"
    local DEFAULT="${2:-y}" # Default to yes

    if is_interactive; then
        if [[ "$DEFAULT" == "y" ]]; then
            read -p "$MESSAGE (Y/n) " -n 1 -r
        else
            read -p "$MESSAGE (y/N) " -n 1 -r
        fi
        echo

        if [[ "$DEFAULT" == "y" ]]; then
            [[ ! $REPLY =~ ^[Nn]$ ]]
        else
            [[ $REPLY =~ ^[Yy]$ ]]
        fi
    else
        [[ "$DEFAULT" == "y" ]]
    fi
}

# Require something or fail/prompt
# Usage: ralph_require "DATABASE_URL" "Add to .env or configure in .ralph/config.yaml"
ralph_require() {
    local WHAT="$1"
    local HINT="$2"

    echo ""
    echo "┌─────────────────────────────────────────────────────────────┐"
    echo "│  MISSING: $WHAT"
    echo "├─────────────────────────────────────────────────────────────┤"
    echo "│  $HINT"
    echo "└─────────────────────────────────────────────────────────────┘"
    echo ""

    if is_interactive; then
        if ralph_confirm "Would you like to provide it now?"; then
            local VALUE=$(ralph_prompt "Enter $WHAT")
            if [[ -n "$VALUE" ]]; then
                export "$WHAT"="$VALUE"
                echo "Set $WHAT for this session."
                return 0
            fi
        fi

        if ralph_confirm "Continue without $WHAT? (some features may be skipped)" "n"; then
            return 1 # Continue but indicate missing
        else
            echo "Exiting. Please configure $WHAT and retry."
            exit 1
        fi
    else
        echo "ERROR: $WHAT is required but not set."
        echo "Hint: $HINT"
        exit 1
    fi
}

# Warning that can pause execution
# Usage: ralph_warn "Database isolation disabled - tests may conflict"
ralph_warn() {
    local MESSAGE="$1"

    echo ""
    echo "⚠️  WARNING: $MESSAGE"
    echo ""

    if is_interactive; then
        if ! ralph_confirm "Continue anyway?"; then
            echo "Exiting at user request."
            exit 0
        fi
    fi
    # In non-interactive mode, warnings just print and continue
}

# Export functions
export -f is_interactive ralph_prompt ralph_confirm ralph_require ralph_warn
