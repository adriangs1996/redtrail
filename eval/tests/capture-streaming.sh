#!/usr/bin/env bash
# Live test: verify DB has partial stdout while a foreground command is still running
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

# Background checker: query DB after 3s while foreground command is still running.
# Runs outside the script session so it doesn't interfere with capture.
(
  sleep 3
  REDTRAIL_DB="$TMPDIR/test.db" "$RT" query \
    "SELECT status, stdout FROM commands WHERE command_raw LIKE '%stream-line-%' LIMIT 1" \
    --json > "$TMPDIR/mid-check.json" 2>/dev/null || true
) &
CHECKER_PID=$!

# Foreground command produces output over 5 seconds (tee stays alive the whole time)
cat >"$TMPDIR/commands.txt" <<'CMDS'
for i in 1 2 3 4 5; do echo "stream-line-$i"; sleep 1; done
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true
wait $CHECKER_PID 2>/dev/null || true

# Mid-execution: should have some output already (tee flushes every 1s)
if [ -f "$TMPDIR/mid-check.json" ]; then
  grep -q "stream-line-" "$TMPDIR/mid-check.json" || {
    echo "FAIL: no partial stdout found mid-execution. Got: $(cat "$TMPDIR/mid-check.json")"
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
