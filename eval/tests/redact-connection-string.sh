#!/usr/bin/env bash
# Live test: connection string with embedded password is redacted before storage
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
echo "postgresql://admin:s3cretP4ss@db.example.com:5432/mydb"
exit 0
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Check command_raw is redacted
CMD_RAW=$("$RT" query "SELECT command_raw FROM commands WHERE command_binary = 'echo'" --json 2>/dev/null)
echo "$CMD_RAW" | grep -q '\[REDACTED:connection_password\]' || {
  echo "FAIL: command_raw not redacted. Got: $CMD_RAW"
  exit 1
}
echo "$CMD_RAW" | grep -q 's3cretP4ss' && {
  echo "FAIL: password still present in command_raw. Got: $CMD_RAW"
  exit 1
}

# Check stdout is redacted
STDOUT_CHECK=$("$RT" query "SELECT stdout FROM commands WHERE command_binary = 'echo' AND stdout IS NOT NULL" --json 2>/dev/null)
echo "$STDOUT_CHECK" | grep -q '\[REDACTED:connection_password\]' || {
  echo "FAIL: stdout not redacted. Got: $STDOUT_CHECK"
  exit 1
}
echo "$STDOUT_CHECK" | grep -q 's3cretP4ss' && {
  echo "FAIL: password still present in stdout. Got: $STDOUT_CHECK"
  exit 1
}

# Check redaction audit log
CMD_ID=$("$RT" query "SELECT id FROM commands WHERE command_binary = 'echo'" --json 2>/dev/null | grep -o '"id": *"[^"]*"' | head -1 | sed 's/.*"id": *"//;s/"//')
AUDIT=$("$RT" query "SELECT field, pattern_label FROM redaction_log WHERE command_id = '$CMD_ID'" --json 2>/dev/null)
echo "$AUDIT" | grep -q '"pattern_label":"connection_password"' || echo "$AUDIT" | grep -q '"pattern_label": "connection_password"' || {
  echo "FAIL: redaction_log missing connection_password entry. Got: $AUDIT"
  exit 1
}
echo "$AUDIT" | grep -q '"field":"command_raw"' || echo "$AUDIT" | grep -q '"field": "command_raw"' || {
  echo "FAIL: redaction_log missing command_raw field entry. Got: $AUDIT"
  exit 1
}
echo "$AUDIT" | grep -q '"field":"stdout"' || echo "$AUDIT" | grep -q '"field": "stdout"' || {
  echo "FAIL: redaction_log missing stdout field entry. Got: $AUDIT"
  exit 1
}

# Verify command status is 'finished'
STATUS_CHECK=$("$RT" query "SELECT status FROM commands WHERE command_binary = 'echo' LIMIT 1" --json 2>/dev/null)
echo "$STATUS_CHECK" | grep -q "finished" || {
  echo "FAIL: command status not 'finished'. Got: $STATUS_CHECK"
  exit 1
}

echo "PASS"
