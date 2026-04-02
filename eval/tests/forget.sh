#!/usr/bin/env bash
# Live test: capture commands, forget them, verify they're gone
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

# Insert commands via capture start/finish (new CLI)
SESSION_ID=$("$RT" session-id 2>/dev/null)

CMD1=$("$RT" capture start \
    --session-id "$SESSION_ID" \
    --command "echo first" \
    --shell zsh \
    --hostname testbox \
    2>/dev/null)
"$RT" capture finish --command-id "$CMD1" --exit-code 0 2>/dev/null

CMD2=$("$RT" capture start \
    --session-id "$SESSION_ID" \
    --command "echo second" \
    --shell zsh \
    --hostname testbox \
    2>/dev/null)
"$RT" capture finish --command-id "$CMD2" --exit-code 0 2>/dev/null

# Verify both exist
COUNT=$("$RT" query "SELECT count(*) as cnt FROM commands" --json 2>/dev/null)
echo "$COUNT" | grep -qE '"cnt":\s*2' || { echo "FAIL: expected 2 commands, got: $COUNT"; exit 1; }

# Forget the session
"$RT" forget --session "$SESSION_ID" 2>/dev/null

# Verify commands are gone
AFTER=$("$RT" query "SELECT count(*) as cnt FROM commands" --json 2>/dev/null)
echo "$AFTER" | grep -qE '"cnt":\s*0' || { echo "FAIL: commands should be deleted, got: $AFTER"; exit 1; }

echo "PASS"
