#!/usr/bin/env bash
# Live test: custom blacklist_commands from config replaces defaults
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"
export REDTRAIL_CONFIG="$TMPDIR/config.yaml"

# Custom blacklist: block "ls" but NOT the default-blacklisted "vim"
# (shell hooks have their own inline blacklist for vim, so we use a
#  command that the shell hook allows but the config would block)
cat >"$TMPDIR/config.yaml" <<'EOF'
capture:
  blacklist_commands:
    - cat
EOF

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

cat >"$TMPDIR/commands.txt" <<'CMDS'
cat /dev/null
echo "this should be captured"
exit 0
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# "cat" should be blocked by custom blacklist
CAT_COUNT=$("$RT" query "SELECT count(*) as cnt FROM commands WHERE command_binary = 'cat'" --json 2>/dev/null)
echo "$CAT_COUNT" | grep -q '"cnt": 0' || echo "$CAT_COUNT" | grep -q '"cnt":0' || {
  echo "FAIL: cat should be blacklisted via config. Got: $CAT_COUNT"
  exit 1
}

# "echo" should still be captured
ECHO_COUNT=$("$RT" query "SELECT count(*) as cnt FROM commands WHERE command_binary = 'echo'" --json 2>/dev/null)
echo "$ECHO_COUNT" | grep -q '"cnt": 1' || echo "$ECHO_COUNT" | grep -q '"cnt":1' || {
  echo "FAIL: echo should be captured. Got: $ECHO_COUNT"
  exit 1
}

# Verify echo command status is 'finished'
STATUS_CHECK=$("$RT" query "SELECT status FROM commands WHERE command_binary = 'echo' LIMIT 1" --json 2>/dev/null)
echo "$STATUS_CHECK" | grep -q "finished" || {
  echo "FAIL: command status not 'finished'. Got: $STATUS_CHECK"
  exit 1
}

echo "PASS"
