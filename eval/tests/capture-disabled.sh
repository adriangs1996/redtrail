#!/usr/bin/env bash
# Live test: capture.enabled=false — commands run fine but nothing is stored
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"
export REDTRAIL_CONFIG="$TMPDIR/config.yaml"

# Disable capture via config
cat >"$TMPDIR/config.yaml" <<'EOF'
capture:
  enabled: false
EOF

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

cat >"$TMPDIR/commands.txt" <<'CMDS'
echo "this should not be captured"
ls /tmp
exit 0
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# DB should have zero commands
COUNT=$("$RT" query "SELECT count(*) as cnt FROM commands" --json 2>/dev/null)
echo "$COUNT" | grep -q '"cnt": 0' || echo "$COUNT" | grep -q '"cnt":0' || {
  echo "FAIL: capture.enabled=false should store nothing. Got: $COUNT"
  exit 1
}

echo "PASS"
