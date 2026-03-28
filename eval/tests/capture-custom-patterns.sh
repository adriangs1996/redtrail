#!/usr/bin/env bash
# Live test: secrets.patterns_file loads custom regex patterns for redaction
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"
export REDTRAIL_CONFIG="$TMPDIR/config.yaml"

# Create a custom patterns file that matches our proprietary token format
cat >"$TMPDIR/patterns.yaml" <<'EOF'
- label: acme_token
  pattern: "ACME-[a-f0-9]{16}"
EOF

cat >"$TMPDIR/config.yaml" <<EOF
secrets:
  patterns_file: $TMPDIR/patterns.yaml
EOF

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

cat >"$TMPDIR/commands.txt" <<'CMDS'
echo "token is ACME-abcdef0123456789"
exit 0
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# The custom token should be redacted in command_raw
CMD_RAW=$("$RT" query "SELECT command_raw FROM commands WHERE command_binary = 'echo'" --json 2>/dev/null)
echo "$CMD_RAW" | grep -q '\[REDACTED:acme_token\]' || {
  echo "FAIL: custom pattern should redact acme_token in command_raw. Got: $CMD_RAW"
  exit 1
}
echo "$CMD_RAW" | grep -q 'ACME-abcdef0123456789' && {
  echo "FAIL: raw token should not be in command_raw. Got: $CMD_RAW"
  exit 1
}

# The custom token should be redacted in stdout too
STDOUT_CHECK=$("$RT" query "SELECT stdout FROM commands WHERE command_binary = 'echo' AND stdout IS NOT NULL" --json 2>/dev/null)
echo "$STDOUT_CHECK" | grep -q '\[REDACTED:acme_token\]' || {
  echo "FAIL: custom pattern should redact acme_token in stdout. Got: $STDOUT_CHECK"
  exit 1
}

# Audit log should have the custom label
CMD_ID=$("$RT" query "SELECT id FROM commands WHERE command_binary = 'echo'" --json 2>/dev/null | grep -o '"id": *"[^"]*"' | head -1 | sed 's/.*"id": *"//;s/"//')
AUDIT=$("$RT" query "SELECT pattern_label FROM redaction_log WHERE command_id = '$CMD_ID'" --json 2>/dev/null)
echo "$AUDIT" | grep -q 'acme_token' || {
  echo "FAIL: redaction_log should have acme_token entry. Got: $AUDIT"
  exit 1
}

echo "PASS"
