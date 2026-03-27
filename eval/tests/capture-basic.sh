#!/usr/bin/env bash
# Live test: source zsh hooks, run a command, verify it appears in history
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

# .zshrc that sources our hooks
cat > "$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
EOF

# Commands to feed to the interactive shell
cat > "$TMPDIR/commands.txt" <<'EOF'
echo "hello from live test"
sleep 1
exit
EOF

# `script` creates a real PTY so preexec/precmd fire properly
HOME="$TMPDIR" script -q -c "zsh -i" /dev/null < "$TMPDIR/commands.txt" > /dev/null 2>&1 || true

# Give background capture a moment
sleep 1

# Verify: the command should appear in history
HISTORY=$("$RT" history --json 2>/dev/null)
echo "$HISTORY" | grep -q "echo" || { echo "FAIL: command not found in history"; exit 1; }

# Verify session was created (check for any session with commands)
SESSIONS=$("$RT" sessions 2>/dev/null)
echo "$SESSIONS" | grep -q "cmds:" || { echo "FAIL: session not found"; exit 1; }

echo "PASS"
