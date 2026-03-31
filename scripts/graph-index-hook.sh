#!/usr/bin/env bash
# Git post-commit hook: incremental graph reindex for the affected project.
# Runs in background (non-blocking). Detects project from repo path.
#
# Install: symlink or copy to .git/hooks/post-commit in each project repo,
# or install globally via git config core.hooksPath.
#
# Usage as standalone: ./graph-index-hook.sh [project-name]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Resolve SIGIL_ROOT: if SCRIPT_DIR is inside a worktree (no config/ sibling),
# walk up from the git common dir to find the real repo root.
if [ -n "${SIGIL_ROOT:-}" ]; then
    : # already set
elif [ -d "$SCRIPT_DIR/../config" ]; then
    SIGIL_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
else
    # We're likely running from a worktree's hooks dir or core.hooksPath.
    # git-common-dir gives the main .git dir (works in both worktrees & main repo).
    _git_common="$(git rev-parse --git-common-dir 2>/dev/null)" || true
    if [ -n "$_git_common" ]; then
        SIGIL_ROOT="$(cd "$_git_common/.." && pwd)"
    else
        SIGIL_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
    fi
fi

# Re-point SCRIPT_DIR to the canonical scripts/ inside SIGIL_ROOT so that
# detect-project.sh and other helpers are always found.
SCRIPT_DIR="$SIGIL_ROOT/scripts"

CONFIG="${SIGIL_CONFIG:-$SIGIL_ROOT/config/sigil.toml}"
DATA_DIR="${SIGIL_DATA_DIR:-$HOME/.sigil}"

# Resolve project name: from arg, from detect-project.sh, or from repo dirname.
PROJECT="${1:-}"
if [ -z "$PROJECT" ] && [ -f "$SCRIPT_DIR/detect-project.sh" ]; then
    PROJECT=$(bash "$SCRIPT_DIR/detect-project.sh" 2>/dev/null) || true
fi
if [ -z "$PROJECT" ]; then
    # --show-toplevel returns the worktree root in a worktree, fall back to git-common-dir parent.
    PROJECT=$(basename "$(git rev-parse --show-toplevel 2>/dev/null)" 2>/dev/null) || true
fi

[ -z "$PROJECT" ] && exit 0

GRAPH_DIR="$DATA_DIR/codegraph"
DB_PATH="$GRAPH_DIR/$PROJECT.db"

# Only index if a graph DB already exists for this project (don't create on first commit).
[ -f "$DB_PATH" ] || exit 0

# Try the CLI first (preferred — uses config for repo path resolution).
if command -v sigil &>/dev/null; then
    sigil graph index -r "$PROJECT" 2>/dev/null &
    disown
    exit 0
fi

# Fallback: direct sigil-graph indexing via the MCP binary isn't available,
# so just touch a marker file that the session primer can pick up.
mkdir -p "$GRAPH_DIR"
echo "$(date -u +%s)" > "$GRAPH_DIR/$PROJECT.stale"
