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

# First: run a command to set up the DB and get a session ID
cat >"$TMPDIR/commands.txt" <<'CMDS'
echo setup-command
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Get the session ID
SESSION_ID=$("$RT" query "SELECT session_id FROM commands LIMIT 1" --json 2>/dev/null | sed -n 's/.*"session_id"\s*:\s*"\([^"]*\)".*/\1/p' | head -1)

if [ -z "$SESSION_ID" ]; then
  echo "FAIL: could not get session ID"
  exit 1
fi

# Insert a stale running command manually (>24h old)
OLD_TS=$(( $(date +%s) - 100000 ))
"$RT" query "INSERT INTO commands (id, session_id, timestamp_start, command_raw, source, status) VALUES ('stale-orphan-test', '$SESSION_ID', $OLD_TS, 'stale command', 'human', 'running')" 2>/dev/null || true

# Run another command — this triggers capture start which runs orphan cleanup
cat >"$TMPDIR/commands2.txt" <<'CMDS'
echo trigger-cleanup
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands2.txt" >/dev/null 2>&1 || true

# Check the stale command
STATUS=$("$RT" query "SELECT status FROM commands WHERE id = 'stale-orphan-test'" --json 2>/dev/null)
echo "$STATUS" | grep -q "orphaned" || {
  echo "FAIL: stale command not marked orphaned. Got: $STATUS"
  exit 1
}

echo "PASS"
