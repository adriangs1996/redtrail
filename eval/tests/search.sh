#!/usr/bin/env bash
# Live test: capture commands with output, verify history --search finds them
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

# Run commands that produce searchable output via real shell hooks + tee
cat >"$TMPDIR/commands.txt" <<'CMDS'
echo "MIGRATION_STATUS: all migrations applied successfully"
echo "git-status-clean"
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Search for text in the command string
SEARCH_CMD=$("$RT" history --search "MIGRATION_STATUS" --json 2>/dev/null)
echo "$SEARCH_CMD" | grep -q "MIGRATION_STATUS" || { echo "FAIL: search for 'MIGRATION_STATUS' should find the echo command. Got: $SEARCH_CMD"; exit 1; }

# Search for text in stdout content
SEARCH_OUT=$("$RT" history --search "migrations" --json 2>/dev/null)
echo "$SEARCH_OUT" | grep -q "MIGRATION_STATUS" || { echo "FAIL: search for 'migrations' (in stdout) should find the echo command. Got: $SEARCH_OUT"; exit 1; }

# Search that doesn't match anything
SEARCH_NONE=$("$RT" history --search "nonexistent_xyz_term" --json 2>/dev/null)
echo "$SEARCH_NONE" | grep -q "MIGRATION_STATUS" && { echo "FAIL: search for nonexistent term should not find anything"; exit 1; }

echo "PASS"
