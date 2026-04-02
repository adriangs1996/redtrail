#!/usr/bin/env bash
# Live test: source bash hooks, run a command, verify it appears in history
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

# .bashrc that sources our hooks
cat >"$TMPDIR/.bashrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init bash)"
EOF

# Commands to feed to the interactive shell
cat >"$TMPDIR/commands.txt" <<'EOF'
echo "hello from live test"
exit
EOF

# `script` creates a real PTY so the DEBUG trap / PROMPT_COMMAND fire properly
HOME="$TMPDIR" script -q -c "bash -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Give background capture a moment
sleep 1

# Verify: the command should appear in history
HISTORY=$("$RT" history --json 2>/dev/null)
echo "$HISTORY" | grep -q "echo" || {
  echo "FAIL: command not found in history"
  exit 1
}

# Verify session was created (check for any session with commands)
SESSIONS=$("$RT" sessions 2>/dev/null)
echo "$SESSIONS" | grep -q "cmds:" || {
  echo "FAIL: session not found"
  exit 1
}

# Verify command status is 'finished'
STATUS_CHECK=$("$RT" query "SELECT status FROM commands WHERE command_binary = 'echo' LIMIT 1" --json 2>/dev/null)
echo "$STATUS_CHECK" | grep -q "finished" || {
  echo "FAIL: command status not 'finished'. Got: $STATUS_CHECK"
  exit 1
}

echo "PASS"
