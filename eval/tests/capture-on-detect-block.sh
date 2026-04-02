#!/usr/bin/env bash
# Live test: secrets.on_detect=block refuses to store commands containing secrets
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"
export REDTRAIL_CONFIG="$TMPDIR/config.yaml"

cat >"$TMPDIR/config.yaml" <<'EOF'
secrets:
  on_detect: block
EOF

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

cat >"$TMPDIR/commands.txt" <<'CMDS'
echo "export AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE"
echo "this is a clean command"
exit 0
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# The command with the secret should NOT be stored
SECRET_COUNT=$("$RT" query "SELECT count(*) as cnt FROM commands WHERE command_raw LIKE '%AKIAIOSFODNN7EXAMPLE%'" --json 2>/dev/null)
echo "$SECRET_COUNT" | grep -q '"cnt": 0' || echo "$SECRET_COUNT" | grep -q '"cnt":0' || {
  echo "FAIL: block mode should not store commands with secrets. Got: $SECRET_COUNT"
  exit 1
}

# The clean command should still be stored
CLEAN_COUNT=$("$RT" query "SELECT count(*) as cnt FROM commands WHERE command_raw LIKE '%clean command%'" --json 2>/dev/null)
echo "$CLEAN_COUNT" | grep -q '"cnt": 1' || echo "$CLEAN_COUNT" | grep -q '"cnt":1' || {
  echo "FAIL: clean command should be stored in block mode. Got: $CLEAN_COUNT"
  exit 1
}

# Verify clean command status is 'finished'
STATUS_CHECK=$("$RT" query "SELECT status FROM commands WHERE command_raw LIKE '%clean command%' LIMIT 1" --json 2>/dev/null)
echo "$STATUS_CHECK" | grep -q "finished" || {
  echo "FAIL: clean command status not 'finished'. Got: $STATUS_CHECK"
  exit 1
}

echo "PASS"
