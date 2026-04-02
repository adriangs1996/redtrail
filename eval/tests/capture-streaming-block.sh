#!/usr/bin/env bash
# Live test: on_detect=block deletes command row when secret appears in stream
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

mkdir -p "$TMPDIR/.config/redtrail"
cat >"$TMPDIR/.config/redtrail/config.yaml" <<'CONF'
secrets:
  on_detect: block
CONF

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

cat >"$TMPDIR/commands.txt" <<'CMDS'
{ sleep 1; echo "key=AKIAIOSFODNN7EXAMPLE"; sleep 2; } &
BGPID=$!
wait $BGPID 2>/dev/null
sleep 3
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# The command row should be deleted (on_detect=block)
COUNT=$("$RT" query "SELECT COUNT(*) as cnt FROM commands WHERE command_raw LIKE '%AKIA%' OR stdout LIKE '%AKIA%'" --json 2>/dev/null)
echo "$COUNT" | grep -qE '"cnt":\s*0' || {
  echo "FAIL: command with secret should have been deleted. Got: $COUNT"
  exit 1
}

echo "PASS"
