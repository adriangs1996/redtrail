#!/usr/bin/env bash
# Live test: source zsh hooks, run a command that produces output,
# verify stdout is captured in the DB via the real tee pipeline
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

cat > "$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
# Force exit to ignore background jobs warning
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

cat > "$TMPDIR/commands.txt" <<'EOF'
echo "captured output line one"
sleep 1
exit 0
EOF

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null < "$TMPDIR/commands.txt" > /dev/null 2>&1 || true

sleep 2

# Verify stdout was captured in the DB
STDOUT_CHECK=$("$RT" query "SELECT stdout FROM commands WHERE command_binary = 'echo' AND stdout IS NOT NULL" --json 2>/dev/null)
echo "$STDOUT_CHECK" | grep -q "captured output line one" || { echo "FAIL: stdout not captured. Got: $STDOUT_CHECK"; exit 1; }

echo "PASS"
