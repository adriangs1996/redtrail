#!/usr/bin/env bash
# Live test: source bash hooks, run a command that produces output,
# verify stdout is captured in the DB via the real tee pipeline
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

cat >"$TMPDIR/.bashrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init bash)"
EOF

cat >"$TMPDIR/commands.txt" <<'EOF'
echo "captured output line one"
exit 0
EOF

HOME="$TMPDIR" script -q -c "bash -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Verify stdout was captured in the DB
STDOUT_CHECK=$("$RT" query "SELECT stdout FROM commands WHERE command_binary = 'echo' AND stdout IS NOT NULL" --json 2>/dev/null)
echo "$STDOUT_CHECK" | grep -q "captured output line one" || {
  echo "FAIL: stdout not captured. Got: $STDOUT_CHECK"
  exit 1
}

# Verify command status is 'finished'
STATUS_CHECK=$("$RT" query "SELECT status FROM commands WHERE command_binary = 'echo' AND stdout IS NOT NULL LIMIT 1" --json 2>/dev/null)
echo "$STATUS_CHECK" | grep -q "finished" || {
  echo "FAIL: command status not 'finished'. Got: $STATUS_CHECK"
  exit 1
}

echo "PASS"
