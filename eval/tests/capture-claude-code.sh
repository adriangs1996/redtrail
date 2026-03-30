#!/usr/bin/env bash
# Live test: install Claude Code hooks, run a prompt that triggers tool use,
# verify that tool executions are captured in the redtrail database.
#
# Requires: CLAUDE_CODE_OAUTH_TOKEN set in environment.
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

# Skip if no OAuth token available
if [ -z "${CLAUDE_CODE_OAUTH_TOKEN:-}" ]; then
  echo "SKIP: CLAUDE_CODE_OAUTH_TOKEN not set"
  exit 0
fi

# Create a workspace directory for Claude Code to operate in
WORKSPACE="$TMPDIR/workspace"
mkdir -p "$WORKSPACE"
cd "$WORKSPACE"

# Initialize git repo (Claude Code expects this)
git init -q
git config user.email "test@test.com"
git config user.name "Test"

# Install redtrail hooks for Claude Code
"$RT" setup-hooks

# Verify hooks were installed
[ -f ".claude/hooks/redtrail-capture.sh" ] || {
  echo "FAIL: hook script not created"
  exit 1
}
grep -q "redtrail" .claude/settings.json || {
  echo "FAIL: settings.json missing redtrail hooks"
  exit 1
}

# Create a file for Claude to read (deterministic tool use)
cat > "$WORKSPACE/test-file.txt" <<'EOF'
This is a test file for redtrail capture verification.
Line two of the test file.
EOF

# Run Claude Code with a prompt that forces tool use.
# --print mode runs non-interactively. The prompt asks Claude to read
# a specific file, which guarantees a Read tool invocation.
claude -p \
  --dangerously-skip-permissions \
  "Read the file test-file.txt and tell me how many lines it has. Do not use Bash, use the Read tool." \
  > /dev/null 2>&1 || {
  echo "FAIL: claude command failed"
  exit 1
}

# Give async hooks a moment to flush
sleep 2

# Verify: tool executions should appear in the database
COUNT=$("$RT" query "SELECT COUNT(*) FROM commands WHERE source = 'claude_code'" 2>/dev/null | tail -1 | tr -d '[:space:]')
if [ "$COUNT" -lt 1 ]; then
  echo "FAIL: no claude_code events in database (count=$COUNT)"
  echo "DEBUG: all commands:"
  "$RT" query "SELECT id, source, tool_name, command_raw FROM commands" 2>/dev/null || true
  exit 1
fi

# Verify: at least one Read tool event should exist
READ_COUNT=$("$RT" query "SELECT COUNT(*) FROM commands WHERE source = 'claude_code' AND tool_name = 'Read'" 2>/dev/null | tail -1 | tr -d '[:space:]')
if [ "$READ_COUNT" -lt 1 ]; then
  echo "FAIL: no Read tool events captured (read_count=$READ_COUNT)"
  echo "DEBUG: captured tools:"
  "$RT" query "SELECT tool_name, command_raw FROM commands WHERE source = 'claude_code'" 2>/dev/null || true
  exit 1
fi

# Verify: an agent session should exist
SESSION_COUNT=$("$RT" query "SELECT COUNT(*) FROM sessions WHERE source = 'claude_code'" 2>/dev/null | tail -1 | tr -d '[:space:]')
if [ "$SESSION_COUNT" -lt 1 ]; then
  echo "FAIL: no claude_code session created"
  exit 1
fi

# Verify: the Read event references the correct file
"$RT" query "SELECT command_raw FROM commands WHERE source = 'claude_code' AND tool_name = 'Read'" 2>/dev/null | grep -q "test-file.txt" || {
  echo "FAIL: Read event does not reference test-file.txt"
  echo "DEBUG:"
  "$RT" query "SELECT command_raw FROM commands WHERE source = 'claude_code'" 2>/dev/null || true
  exit 1
}

echo "PASS: captured $COUNT claude_code event(s), including $READ_COUNT Read tool call(s)"
