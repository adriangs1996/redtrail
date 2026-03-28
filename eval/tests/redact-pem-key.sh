#!/usr/bin/env bash
# Live test: `echo <pem> > key.pem` — the real file must contain the key,
# but the DB (command_raw, stdout) must have it redacted.
set -euo pipefail

RT="/usr/local/bin/redtrail"
PEM_BODY="MIIEpAIBAAKCAQEA0Z3VS5JJcds3xfn"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

# Realistic workflow: write a PEM key to a file via redirect
cat >"$TMPDIR/commands.txt" <<CMDS
echo "-----BEGIN RSA PRIVATE KEY-----
${PEM_BODY}
-----END RSA PRIVATE KEY-----" > $TMPDIR/key.pem
exit 0
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# ── 1. The actual file on disk must have the real key ──
test -f "$TMPDIR/key.pem" || { echo "FAIL: key.pem was not created"; exit 1; }
grep -q 'BEGIN RSA PRIVATE KEY' "$TMPDIR/key.pem" || {
  echo "FAIL: key.pem should contain the real PEM header. Got:"
  cat "$TMPDIR/key.pem"
  exit 1
}
grep -q "$PEM_BODY" "$TMPDIR/key.pem" || {
  echo "FAIL: key.pem should contain the real key body. Got:"
  cat "$TMPDIR/key.pem"
  exit 1
}

# ── 2. command_raw in DB is redacted ──
CMD_RAW=$("$RT" query "SELECT command_raw FROM commands WHERE command_binary = 'echo'" --json 2>/dev/null)
echo "$CMD_RAW" | grep -q '\[REDACTED:private_key\]' || {
  echo "FAIL: command_raw not redacted. Got: $CMD_RAW"
  exit 1
}
echo "$CMD_RAW" | grep -q 'BEGIN RSA PRIVATE KEY' && {
  echo "FAIL: PEM header still present in command_raw. Got: $CMD_RAW"
  exit 1
}

# ── 3. stdout in DB is redacted (redirect suppresses stdout, so it may be
#       empty — only assert if stdout was captured) ──
STDOUT_CHECK=$("$RT" query "SELECT stdout FROM commands WHERE command_binary = 'echo' AND stdout IS NOT NULL AND stdout != ''" --json 2>/dev/null)
if echo "$STDOUT_CHECK" | grep -q 'stdout'; then
  echo "$STDOUT_CHECK" | grep -q 'BEGIN RSA PRIVATE KEY' && {
    echo "FAIL: PEM header still present in stdout. Got: $STDOUT_CHECK"
    exit 1
  }
fi

# ── 4. Redaction audit log has entries ──
CMD_ID=$("$RT" query "SELECT id FROM commands WHERE command_binary = 'echo'" --json 2>/dev/null | grep -o '"id": *"[^"]*"' | head -1 | sed 's/.*"id": *"//;s/"//')
AUDIT=$("$RT" query "SELECT field, pattern_label FROM redaction_log WHERE command_id = '$CMD_ID'" --json 2>/dev/null)
echo "$AUDIT" | grep -q '"pattern_label":"private_key"' || echo "$AUDIT" | grep -q '"pattern_label": "private_key"' || {
  echo "FAIL: redaction_log missing private_key entry. Got: $AUDIT"
  exit 1
}
echo "$AUDIT" | grep -q '"field":"command_raw"' || echo "$AUDIT" | grep -q '"field": "command_raw"' || {
  echo "FAIL: redaction_log missing command_raw field entry. Got: $AUDIT"
  exit 1
}

echo "PASS"
