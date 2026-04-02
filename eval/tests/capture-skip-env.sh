#!/usr/bin/env bash
# Live test: REDTRAIL_SKIP=1 env var skips capture for a single command
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"
export REDTRAIL_CONFIG="$TMPDIR/config.yaml"

cat >"$TMPDIR/config.yaml" <<'EOF'
capture:
  blacklist_commands: []
EOF

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

cat >"$TMPDIR/commands.txt" <<'CMDS'
echo "captured-normal"
sleep 1
REDTRAIL_SKIP=1 echo "skipped-by-env"
sleep 1
echo "captured-after-skip"
exit 0
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# The skipped command should NOT be in the database
SKIP_COUNT=$("$RT" query "SELECT count(*) as cnt FROM commands WHERE command_raw LIKE '%skipped-by-env%'" --json 2>/dev/null)
echo "$SKIP_COUNT" | grep -q '"cnt": 0' || echo "$SKIP_COUNT" | grep -q '"cnt":0' || {
  echo "FAIL: REDTRAIL_SKIP=1 command should not be captured. Got: $SKIP_COUNT"
  exit 1
}

# Normal commands should still be captured
NORMAL_COUNT=$("$RT" query "SELECT count(*) as cnt FROM commands WHERE command_raw LIKE '%captured-normal%'" --json 2>/dev/null)
echo "$NORMAL_COUNT" | grep -q '"cnt": 1' || echo "$NORMAL_COUNT" | grep -q '"cnt":1' || {
  echo "FAIL: normal echo should be captured. Got: $NORMAL_COUNT"
  exit 1
}

# Command after skip should also be captured (skip doesn't persist)
AFTER_COUNT=$("$RT" query "SELECT count(*) as cnt FROM commands WHERE command_raw LIKE '%captured-after-skip%'" --json 2>/dev/null)
echo "$AFTER_COUNT" | grep -q '"cnt": 1' || echo "$AFTER_COUNT" | grep -q '"cnt":1' || {
  echo "FAIL: command after skip should be captured. Got: $AFTER_COUNT"
  exit 1
}

# Verify captured commands have status 'finished'
STATUS_CHECK=$("$RT" query "SELECT status FROM commands WHERE command_raw LIKE '%captured-normal%' LIMIT 1" --json 2>/dev/null)
echo "$STATUS_CHECK" | grep -q "finished" || {
  echo "FAIL: command status not 'finished'. Got: $STATUS_CHECK"
  exit 1
}

echo "PASS"
