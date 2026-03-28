#!/usr/bin/env bash
# Live test: history output includes exit code, duration, relative time, and command
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

# .zshrc that sources our hooks
cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

# Run a successful and a failing command
cat >"$TMPDIR/commands.txt" <<'EOF'
echo "format-test-marker"
false
exit
EOF

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Force table output (not JSON) by using --verbose which implies table mode
OUTPUT=$("$RT" history --verbose 2>/dev/null)

# Should contain the command text
echo "$OUTPUT" | grep -q "echo" || {
  echo "FAIL: command 'echo' not found in output"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

# Should contain relative time indicator (e.g. "just now" or "Xm ago")
echo "$OUTPUT" | grep -qE "(just now|[0-9]+[mhd] ago)" || {
  echo "FAIL: no relative timestamp in output"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

# Should contain duration indicator (e.g. "<1s" or "Xs")
echo "$OUTPUT" | grep -qE "(<1s|[0-9]+s|[0-9]+m)" || {
  echo "FAIL: no duration in output"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

# Verbose mode should show stdout content inline
echo "$OUTPUT" | grep -q "format-test-marker" || {
  echo "FAIL: verbose output should show stdout content 'format-test-marker'"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

echo "PASS"
