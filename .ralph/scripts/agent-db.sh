#!/bin/bash
# Per-agent database isolation using PostgreSQL template databases
# Usage: source agent-db.sh <create|destroy> <agent_id>
#
# Requires explicit config in .ralph/config.yaml:
#   database:
#     from_env: "DATABASE_URL"  # Read from .env
#   OR
#     url: "postgres://..."     # Direct URL (dev only!)

ACTION=$1
AGENT_ID=$(printf '%03d' "${2:-1}")
CONFIG=".ralph/config.yaml"

# Load prompt helpers if available
if [[ -f ".ralph/scripts/prompt-user.sh" ]]; then
    source .ralph/scripts/prompt-user.sh
fi

# Check if database isolation is configured
if [[ ! -f "$CONFIG" ]]; then
    return 0
fi

DB_FROM_ENV=$(yq '.database.from_env // ""' "$CONFIG" 2> /dev/null)
DB_URL=$(yq '.database.url // ""' "$CONFIG" 2> /dev/null)

# Get DATABASE_URL from configured source
if [[ -n "$DB_FROM_ENV" && "$DB_FROM_ENV" != "null" ]]; then
    # Load from .env file
    if [[ -f ".env" ]]; then
        source .env 2> /dev/null || true
    fi
    DATABASE_URL="${!DB_FROM_ENV}"

    if [[ -z "$DATABASE_URL" ]]; then
        if type ralph_require &> /dev/null; then
            ralph_require "$DB_FROM_ENV" "Add $DB_FROM_ENV to your .env file (LOCAL/DEV database only!)"
            DATABASE_URL="${!DB_FROM_ENV}"
        else
            echo "ERROR: database.from_env='$DB_FROM_ENV' but variable not found in .env"
            return 1
        fi
    fi
elif [[ -n "$DB_URL" && "$DB_URL" != "null" ]]; then
    DATABASE_URL="$DB_URL"
else
    # No database config - skip silently
    return 0
fi

# Skip if still no URL
if [[ -z "$DATABASE_URL" ]]; then
    return 0
fi

# Only support postgres
if [[ "$DATABASE_URL" != postgres://* && "$DATABASE_URL" != postgresql://* ]]; then
    if type ralph_warn &> /dev/null; then
        ralph_warn "DATABASE_URL is not a postgres URL, skipping DB isolation"
    else
        echo "WARNING: DATABASE_URL is not a postgres URL, skipping DB isolation"
    fi
    return 0
fi

# Parse DATABASE_URL: postgres://user:pass@host:port/dbname
# Remove protocol
DB_CONN="${DATABASE_URL#*://}"
# Extract user:pass
DB_AUTH="${DB_CONN%%@*}"
DB_USER="${DB_AUTH%%:*}"
DB_PASS="${DB_AUTH#*:}"
# Extract host:port/dbname
DB_HOSTPATH="${DB_CONN#*@}"
DB_HOSTPORT="${DB_HOSTPATH%%/*}"
DB_HOST="${DB_HOSTPORT%%:*}"
DB_PORT="${DB_HOSTPORT#*:}"
[[ "$DB_PORT" == "$DB_HOST" ]] && DB_PORT="5432"
# Extract dbname (template)
TEMPLATE="${DB_HOSTPATH#*/}"
TEMPLATE="${TEMPLATE%%\?*}" # Remove query params

DB_NAME="ralph_agent_${AGENT_ID}"

export PGPASSWORD="$DB_PASS"

case $ACTION in
    create)
        # Create database from template (instant copy-on-write)
        if createdb -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -T "$TEMPLATE" "$DB_NAME" 2> /dev/null; then
            echo "Created database: $DB_NAME (from $TEMPLATE)"
        else
            echo "Database $DB_NAME already exists or creation failed"
        fi

        # Export DATABASE_URL for the agent to use
        export DATABASE_URL="postgres://${DB_USER}:${DB_PASS}@${DB_HOST}:${DB_PORT}/${DB_NAME}"
        echo "DATABASE_URL=$DATABASE_URL"
        ;;

    destroy)
        # Drop the agent's database
        if dropdb -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" "$DB_NAME" 2> /dev/null; then
            echo "Destroyed database: $DB_NAME"
        else
            echo "Database $DB_NAME does not exist or destruction failed"
        fi

        # Restore original DATABASE_URL (the template)
        export DATABASE_URL="postgres://${DB_USER}:${DB_PASS}@${DB_HOST}:${DB_PORT}/${TEMPLATE}"
        ;;

    *)
        echo "Usage: source agent-db.sh <create|destroy> <agent_id>"
        echo ""
        echo "Requires config in .ralph/config.yaml:"
        echo "  database:"
        echo "    from_env: \"DATABASE_URL\"  # Read from .env"
        echo ""
        echo "WARNING: Only use LOCAL/DEV databases!"
        echo ""
        echo "Examples:"
        echo "  source agent-db.sh create 1   # Create ralph_agent_001 from your DB"
        echo "  source agent-db.sh destroy 1  # Drop ralph_agent_001"
        ;;
esac

unset PGPASSWORD
