#!/usr/bin/env bash
# Live test: capture commands with output, verify history --search finds them
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

SESSION_ID=$("$RT" session-id 2>/dev/null)

# Write a stdout temp file with known content
STDOUT_FILE="$TMPDIR/rt-out-search"
cat > "$STDOUT_FILE" <<'EOF'
ts_start:1000
ts_end:1001
truncated:false

MIGRATION_STATUS: all migrations applied successfully
Tables: users, orders, products
EOF

# Capture a command with stdout containing searchable content
"$RT" capture \
    --session-id "$SESSION_ID" \
    --command "rake db:migrate" \
    --exit-code 0 \
    --shell zsh \
    --hostname testbox \
    --stdout-file "$STDOUT_FILE" \
    2>/dev/null

# Capture another command without output
"$RT" capture \
    --session-id "$SESSION_ID" \
    --command "git status" \
    --exit-code 0 \
    --ts-start 2000 \
    --shell zsh \
    --hostname testbox \
    2>/dev/null

# Search for text in the command string
SEARCH_CMD=$("$RT" history --search "migrate" --json 2>/dev/null)
echo "$SEARCH_CMD" | grep -q "rake" || { echo "FAIL: search for 'migrate' should find the rake command"; exit 1; }

# Search for text in stdout content
SEARCH_OUT=$("$RT" history --search "MIGRATION_STATUS" --json 2>/dev/null)
echo "$SEARCH_OUT" | grep -q "rake" || { echo "FAIL: search for 'MIGRATION_STATUS' (in stdout) should find the rake command"; exit 1; }

# Search that doesn't match anything
SEARCH_NONE=$("$RT" history --search "nonexistent_xyz_term" --json 2>/dev/null)
echo "$SEARCH_NONE" | grep -q "rake" && { echo "FAIL: search for nonexistent term should not find anything"; exit 1; }

echo "PASS"
