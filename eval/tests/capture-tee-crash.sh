#!/usr/bin/env bash
# Live test: tee killed mid-stream — partial stdout preserved, finish still works
# Starts a long-running command, kills the tee process via SIGKILL while it's
# running, then verifies partial stdout from the last successful flush is in DB
# and capture finish still sets status='finished'.
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

# The command outputs lines, then mid-stream we kill tee via $__RT_TEE_PID
# (which is set by preexec). After killing tee, output more lines that
# should NOT appear in DB. Then exit normally — precmd calls capture finish.
cat >"$TMPDIR/commands.txt" <<'CMDS'
echo "tee-crash-line-1"; sleep 1; echo "tee-crash-line-2"; sleep 2; kill -9 $__RT_TEE_PID 2>/dev/null; sleep 1; echo "tee-crash-after-kill"; sleep 1
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Partial stdout from before the kill should be preserved (tee flushed at 1s intervals)
STDOUT_CHECK=$("$RT" query "SELECT stdout FROM commands WHERE command_raw LIKE '%tee-crash-line%' AND stdout IS NOT NULL LIMIT 1" --json 2>/dev/null)

# At least the first line should have been flushed before the kill
echo "$STDOUT_CHECK" | grep -q "tee-crash-line-1" || {
  echo "FAIL: partial stdout not preserved after tee crash. Got: $STDOUT_CHECK"
  exit 1
}

# Lines after the kill should NOT be in DB (tee is dead, can't flush)
echo "$STDOUT_CHECK" | grep -q "tee-crash-after-kill" && {
  echo "FAIL: output after tee kill should not be in DB. Got: $STDOUT_CHECK"
  exit 1
}

# capture finish should still have completed — status should be 'finished'
STATUS=$("$RT" query "SELECT status FROM commands WHERE command_raw LIKE '%tee-crash-line%' LIMIT 1" --json 2>/dev/null)
echo "$STATUS" | grep -q "finished" || {
  echo "FAIL: status not 'finished' after tee crash. Got: $STATUS"
  exit 1
}

echo "PASS"
