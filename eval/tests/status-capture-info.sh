#!/usr/bin/env bash
# Live test: status shows last capture time and capture status
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

# Run a command to populate the DB
cat >"$TMPDIR/commands.txt" <<'EOF'
echo "status-test"
exit
EOF

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

OUTPUT=$("$RT" status 2>/dev/null)

# Should show last capture time
echo "$OUTPUT" | grep -q "Last capture:" || {
  echo "FAIL: status missing 'Last capture:'"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

# Last capture should show "just now" (we just ran a command)
echo "$OUTPUT" | grep -q "just now" || {
  echo "FAIL: last capture should be 'just now'"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

# Should show capture status line
echo "$OUTPUT" | grep -q "Capture:" || {
  echo "FAIL: status missing 'Capture:'"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

echo "PASS"
