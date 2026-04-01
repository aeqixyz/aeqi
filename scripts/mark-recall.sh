#!/usr/bin/env bash
# PostToolUse hook for mcp__sigil__sigil_recall: open the recall gate.
# Writes the recalled project name to the gate file.
# Any subsequent recall for a different project updates the gate.
#
# Hook data arrives on stdin as JSON with tool_input.project.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/hook-log.sh"

# Read hook payload from stdin
PAYLOAD=$(timeout 2 cat 2>/dev/null) || true

PROJECT=""
if [ -n "$PAYLOAD" ]; then
    PROJECT=$(printf '%s' "$PAYLOAD" | jq -r '.tool_input.project // empty' 2>/dev/null) || true
fi
# Legacy env var fallback
if [ -z "$PROJECT" ] && [ -n "${CLAUDE_TOOL_INPUT:-}" ]; then
    PROJECT=$(printf '%s' "$CLAUDE_TOOL_INPUT" | jq -r '.project // empty' 2>/dev/null) || true
fi
[ -z "$PROJECT" ] && PROJECT="sigil"

printf '%s' "$PROJECT" > "$SIGIL_SESSION_DIR/recall.gate"
log_hook "mark-recall" "touch" "project=$PROJECT"
