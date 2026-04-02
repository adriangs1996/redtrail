#!/usr/bin/env bash
# Live test: stale 'running' commands cleaned up by capture start
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

# Step 1: Run a command to create the DB and get a session ID
cat >"$TMPDIR/commands.txt" <<'CMDS'
echo setup-command
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Step 2: Insert a stale running command using sqlite3 directly
# (redtrail query only allows SELECT, so we use sqlite3 for test setup)
SESSION_ID=$("$RT" query "SELECT session_id FROM commands LIMIT 1" --json 2>/dev/null | sed -n 's/.*"session_id" *: *"\([^"]*\)".*/\1/p' | head -1)

if [ -z "$SESSION_ID" ]; then
  echo "FAIL: could not get session ID"
  exit 1
fi

OLD_TS=$(( $(date +%s) - 100000 ))
sqlite3 "$TMPDIR/test.db" \
  "INSERT INTO commands (id, session_id, timestamp_start, command_raw, source, status) VALUES ('stale-orphan-test', '$SESSION_ID', $OLD_TS, 'stale command', 'human', 'running')"

# Verify the stale command exists
BEFORE=$("$RT" query "SELECT status FROM commands WHERE id = 'stale-orphan-test'" --json 2>/dev/null)
echo "$BEFORE" | grep -q "running" || {
  echo "FAIL: stale command should exist with status 'running'. Got: $BEFORE"
  exit 1
}

# Step 3: Run another command — triggers capture start which runs orphan cleanup
cat >"$TMPDIR/commands2.txt" <<'CMDS'
echo trigger-cleanup
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands2.txt" >/dev/null 2>&1 || true

# Check the stale command was marked orphaned
STATUS=$("$RT" query "SELECT status FROM commands WHERE id = 'stale-orphan-test'" --json 2>/dev/null)
echo "$STATUS" | grep -q "orphaned" || {
  echo "FAIL: stale command not marked orphaned. Got: $STATUS"
  exit 1
}

echo "PASS"
