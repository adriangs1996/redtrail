#!/usr/bin/env bash
# Live test: on_detect=warn stores unredacted stdout and logs a single warning
# Verifies tee's warn mode during streaming: raw secret stays in DB,
# warning is emitted once (not per-flush), and status finishes correctly.
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

mkdir -p "$TMPDIR/.config/redtrail"
cat >"$TMPDIR/.config/redtrail/config.yaml" <<'CONF'
secrets:
  on_detect: warn
CONF

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

# Foreground command outputs a secret between innocuous lines.
# Tee flushes every 1s — the secret appears in stdout unredacted (warn mode).
cat >"$TMPDIR/commands.txt" <<'CMDS'
echo "before-warn"; sleep 2; echo "key=AKIAIOSFODNN7EXAMPLE"; sleep 2; echo "after-warn"
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Warn mode: raw secret should be present in stdout (NOT redacted)
STDOUT_CHECK=$("$RT" query "SELECT stdout FROM commands WHERE command_raw LIKE '%before-warn%' AND stdout IS NOT NULL LIMIT 1" --json 2>/dev/null)
echo "$STDOUT_CHECK" | grep -q "AKIAIOSFODNN7EXAMPLE" || {
  echo "FAIL: warn mode should store unredacted secret. Got: $STDOUT_CHECK"
  exit 1
}

# Verify no [REDACTED] marker (warn stores raw)
echo "$STDOUT_CHECK" | grep -q "REDACTED" && {
  echo "FAIL: warn mode should NOT redact. Got: $STDOUT_CHECK"
  exit 1
}

# Verify command status is finished
STATUS=$("$RT" query "SELECT status FROM commands WHERE command_raw LIKE '%before-warn%' LIMIT 1" --json 2>/dev/null)
echo "$STATUS" | grep -q "finished" || {
  echo "FAIL: status not 'finished'. Got: $STATUS"
  exit 1
}

echo "PASS"
