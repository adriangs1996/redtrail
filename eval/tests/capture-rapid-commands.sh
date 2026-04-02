#!/usr/bin/env bash
# Live test: rapid sequential commands all captured correctly
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

cat >"$TMPDIR/commands.txt" <<'CMDS'
echo rapid-1
echo rapid-2
echo rapid-3
echo rapid-4
echo rapid-5
echo rapid-6
echo rapid-7
echo rapid-8
echo rapid-9
echo rapid-10
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# All 10 should be captured and finished
COUNT=$("$RT" query "SELECT COUNT(*) as cnt FROM commands WHERE command_raw LIKE 'echo rapid-%'" --json 2>/dev/null)
# Accept both "cnt":10 and "cnt": 10 (spacing varies)
echo "$COUNT" | grep -qE '"cnt":\s*10' || {
  echo "FAIL: expected 10 rapid commands captured. Got: $COUNT"
  exit 1
}

RUNNING=$("$RT" query "SELECT COUNT(*) as cnt FROM commands WHERE command_raw LIKE 'echo rapid-%' AND status != 'finished'" --json 2>/dev/null)
echo "$RUNNING" | grep -qE '"cnt":\s*0' || {
  echo "FAIL: some rapid commands not finished. Got: $RUNNING"
  exit 1
}

echo "PASS"
