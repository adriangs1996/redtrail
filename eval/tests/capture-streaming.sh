#!/usr/bin/env bash
# Live test: verify DB has partial stdout while a command is still running
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

# The trick: run a background loop that emits lines slowly,
# query DB after 3s while loop is still going, then wait and exit.
cat >"$TMPDIR/commands.txt" <<'CMDS'
{ for i in 1 2 3 4 5; do echo "stream-line-$i"; sleep 1; done; } &
BGPID=$!
sleep 3
/usr/local/bin/redtrail query "SELECT status, stdout FROM commands WHERE command_raw LIKE '%stream-line-%' LIMIT 1" --json > /tmp/rt-mid-check.json 2>/dev/null
wait $BGPID 2>/dev/null
sleep 2
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Mid-execution: should have some output already
if [ -f /tmp/rt-mid-check.json ]; then
  grep -q "stream-line-" /tmp/rt-mid-check.json || {
    echo "FAIL: no partial stdout found mid-execution. Got: $(cat /tmp/rt-mid-check.json)"
    exit 1
  }
else
  echo "FAIL: mid-execution check file not created"
  exit 1
fi

# Final: status should be finished
FINAL=$("$RT" query "SELECT status FROM commands WHERE command_raw LIKE '%stream-line-%' LIMIT 1" --json 2>/dev/null)
echo "$FINAL" | grep -q "finished" || {
  echo "FAIL: final status not 'finished'. Got: $FINAL"
  exit 1
}

echo "PASS"
