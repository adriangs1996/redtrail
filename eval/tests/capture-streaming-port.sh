#!/usr/bin/env bash
# Live test: server port announcement captured in DB during streaming
# Simulates a server that prints a port binding message, then stays alive.
# Verifies tee streams the port announcement to DB while the "server" runs.
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

# Background checker: query DB after 3s while "server" is still running
(
  sleep 3
  REDTRAIL_DB="$TMPDIR/test.db" "$RT" query \
    "SELECT stdout FROM commands WHERE command_raw LIKE '%simulated-server%' AND stdout IS NOT NULL LIMIT 1" \
    --json > "$TMPDIR/mid-check.json" 2>/dev/null || true
) &
CHECKER_PID=$!

# Foreground command simulates a server: prints port binding, then sleeps (still "running")
cat >"$TMPDIR/commands.txt" <<'CMDS'
echo "simulated-server starting..."; sleep 1; echo "Listening on http://0.0.0.0:3000"; sleep 4
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true
wait $CHECKER_PID 2>/dev/null || true

# Mid-execution: DB should have the port announcement from streaming
if [ -f "$TMPDIR/mid-check.json" ]; then
  grep -q "Listening on" "$TMPDIR/mid-check.json" || {
    echo "FAIL: port announcement not found mid-execution. Got: $(cat "$TMPDIR/mid-check.json")"
    exit 1
  }
else
  echo "FAIL: mid-execution check file not created"
  exit 1
fi

# Final: command should be finished
FINAL=$("$RT" query "SELECT status FROM commands WHERE command_raw LIKE '%simulated-server%' LIMIT 1" --json 2>/dev/null)
echo "$FINAL" | grep -q "finished" || {
  echo "FAIL: final status not 'finished'. Got: $FINAL"
  exit 1
}

echo "PASS"
