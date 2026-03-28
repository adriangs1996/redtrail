#!/usr/bin/env bash
# Live test: capture commands, forget them, verify they're gone
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

# Insert commands directly via the capture CLI (no hooks needed for this test)
SESSION_ID=$("$RT" session-id 2>/dev/null)
NOW=$(date +%s)

"$RT" capture \
    --session-id "$SESSION_ID" \
    --command "echo first" \
    --exit-code 0 \
    --ts-start "$NOW" \
    --shell zsh \
    --hostname testbox \
    2>/dev/null

"$RT" capture \
    --session-id "$SESSION_ID" \
    --command "echo second" \
    --exit-code 0 \
    --ts-start "$((NOW + 1))" \
    --shell zsh \
    --hostname testbox \
    2>/dev/null

# Verify both exist
COUNT=$("$RT" query "SELECT count(*) as cnt FROM commands" --json 2>/dev/null)
echo "$COUNT" | grep -q '"cnt":2' || echo "$COUNT" | grep -q '"cnt": 2' || { echo "FAIL: expected 2 commands, got: $COUNT"; exit 1; }

# Forget the session
"$RT" forget --session "$SESSION_ID" 2>/dev/null

# Verify commands are gone
AFTER=$("$RT" query "SELECT count(*) as cnt FROM commands" --json 2>/dev/null)
echo "$AFTER" | grep -q '"cnt":0' || echo "$AFTER" | grep -q '"cnt": 0' || { echo "FAIL: commands should be deleted, got: $AFTER"; exit 1; }

echo "PASS"
