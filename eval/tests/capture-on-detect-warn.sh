#!/usr/bin/env bash
# Live test: secrets.on_detect=warn stores unredacted but flags as redacted
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"
export REDTRAIL_CONFIG="$TMPDIR/config.yaml"

cat >"$TMPDIR/config.yaml" <<'EOF'
secrets:
  on_detect: warn
EOF

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

cat >"$TMPDIR/commands.txt" <<'CMDS'
echo "export AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE"
exit 0
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Command should be stored UNredacted (warn mode doesn't redact)
CMD_RAW=$("$RT" query "SELECT command_raw FROM commands WHERE command_binary = 'echo'" --json 2>/dev/null)
echo "$CMD_RAW" | grep -q 'AKIAIOSFODNN7EXAMPLE' || {
  echo "FAIL: warn mode should store unredacted. Got: $CMD_RAW"
  exit 1
}

# But the redacted flag should be set (indicating detection happened)
REDACTED=$("$RT" query "SELECT redacted FROM commands WHERE command_binary = 'echo'" --json 2>/dev/null)
echo "$REDACTED" | grep -q '"redacted": 1' || echo "$REDACTED" | grep -q '"redacted":1' || {
  echo "FAIL: redacted flag should be 1 in warn mode. Got: $REDACTED"
  exit 1
}

# Verify command status is 'finished'
STATUS_CHECK=$("$RT" query "SELECT status FROM commands WHERE command_binary = 'echo' LIMIT 1" --json 2>/dev/null)
echo "$STATUS_CHECK" | grep -q "finished" || {
  echo "FAIL: command status not 'finished'. Got: $STATUS_CHECK"
  exit 1
}

echo "PASS"
