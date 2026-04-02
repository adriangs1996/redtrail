#!/usr/bin/env bash
# Live test: verify secrets are redacted during streaming (not just at finish)
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

# Foreground command outputs a secret between sleeps — tee is alive the whole time.
# Semicolon-chained so it's one command line (one tee instance).
cat >"$TMPDIR/commands.txt" <<'CMDS'
echo "before-secret"; sleep 2; echo "key=AKIAIOSFODNN7EXAMPLE"; sleep 2; echo "after-secret"
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Check: raw key should NOT be in DB, REDACTED marker should be present
FINAL=$("$RT" query "SELECT stdout FROM commands WHERE stdout IS NOT NULL AND command_raw LIKE '%before-secret%' LIMIT 1" --json 2>/dev/null)

echo "$FINAL" | grep -q "AKIAIOSFODNN7EXAMPLE" && {
  echo "FAIL: raw AWS key found in DB (should be redacted)"
  exit 1
}

echo "$FINAL" | grep -q "REDACTED" || {
  echo "FAIL: no redaction marker found. Got: $FINAL"
  exit 1
}

echo "PASS"
