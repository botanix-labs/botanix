#!/usr/bin/env bash
set -euo pipefail

# ──────────────────────────────────────────────────────────────
#  update-agents.sh — Install/update AI agent skills from botanix-labs/botanix-skills
#
#  Usage:
#    ./scripts/update-agents.sh                  # install for default agents
#    ./scripts/update-agents.sh claude-code       # install for claude-code only
#    ./scripts/update-agents.sh codex cursor      # install for codex and cursor
#    AGENTS="codex" ./scripts/update-agents.sh   # via env var
# ──────────────────────────────────────────────────────────────

SKILLS_REPO="botanix-labs/botanix-skills"
SKILLS_CLI_VERSION="${SKILLS_CLI_VERSION:-1.3.9}"

# Default agents to install for (override via args or AGENTS env var)
DEFAULT_AGENTS=("claude-code" "codex" "cursor" "droid" "opencode" "antigravity" "github-copilot")

# Determine target agents: CLI args > env var > defaults
if [[ $# -gt 0 ]]; then
    AGENTS=("$@")
elif [[ -n "${AGENTS:-}" ]]; then
    read -ra AGENTS <<< "$AGENTS"
else
    AGENTS=("${DEFAULT_AGENTS[@]}")
fi

echo "╔══════════════════════════════════════════════════╗"
echo "║  Updating AI Agent Skills                        ║"
echo "╠══════════════════════════════════════════════════╣"
echo "║  Repo:   ${SKILLS_REPO}"
echo "║  Agents: ${AGENTS[*]}"
echo "╚══════════════════════════════════════════════════╝"
echo ""

# Ensure skills CLI is installed at the pinned version
echo "→ Installing skills CLI v${SKILLS_CLI_VERSION}..."
npm install --no-save "skills@${SKILLS_CLI_VERSION}" 2> /dev/null

# Build the agent flags: -a claude-code -a codex ...
AGENT_FLAGS=()
for agent in "${AGENTS[@]}"; do
    AGENT_FLAGS+=("-a" "$agent")
done

# Install all skills from the repo, globally, non-interactively
echo "→ Installing all skills for: ${AGENTS[*]}..."
npx skills add "${SKILLS_REPO}" \
    --skill '*' \
    "${AGENT_FLAGS[@]}" \
    -y

echo ""
echo "✓ Agent skills updated successfully"
