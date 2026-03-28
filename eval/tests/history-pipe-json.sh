#!/usr/bin/env bash
# Live test: history auto-switches to JSON when piped (stdout not a TTY)
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

# .zshrc
cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

# Run a command to populate history
cat >"$TMPDIR/commands.txt" <<'EOF'
echo "pipe-json-marker"
exit
EOF

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# When piped, output should be valid JSON
OUTPUT=$("$RT" history 2>/dev/null | cat)

# Verify it starts with [ (JSON array) — simple but sufficient
FIRST_CHAR=$(echo "$OUTPUT" | head -c1)
[[ "$FIRST_CHAR" == "[" ]] || {
  echo "FAIL: piped output is not a JSON array (starts with '$FIRST_CHAR')"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

# JSON should contain our command
echo "$OUTPUT" | grep -q "pipe-json-marker" || {
  echo "FAIL: JSON output missing our command"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

echo "PASS"
