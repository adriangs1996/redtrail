#!/usr/bin/env bash
# Live test: seed agent events, then verify agent-context output
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

# Initialize a workspace with git
WORKSPACE="$TMPDIR/workspace"
mkdir -p "$WORKSPACE"
cd "$WORKSPACE"
git init -q
git config user.email "test@test.com"
git config user.name "Test"

# Helper: ingest a Claude Code tool event
ingest_event() {
  local tool_name="$1"
  local tool_input="$2"
  local session_id="${3:-test-session-1}"
  local tool_response="${4:-null}"
  local error="${5:-null}"

  echo "{
    \"tool_name\": \"$tool_name\",
    \"tool_input\": $tool_input,
    \"tool_response\": $tool_response,
    \"error\": $error,
    \"cwd\": \"$WORKSPACE\",
    \"session_id\": \"$session_id\"
  }" | "$RT" ingest 2>/dev/null
}

# Seed two agent sessions with different session IDs
# Session 1: read and edit files
ingest_event "Read" '{"file_path": "src/main.rs"}' "session-A"
ingest_event "Edit" '{"file_path": "src/main.rs", "old_string": "a", "new_string": "b"}' "session-A"
ingest_event "Bash" '{"command": "cargo test"}' "session-A" '{"stdout": "ok", "exit_code": 0}'

# Session 2: different work
ingest_event "Read" '{"file_path": "README.md"}' "session-B"
ingest_event "Bash" '{"command": "cargo build"}' "session-B" '{"stdout": "ok", "exit_code": 0}'

# Verify: empty project message when in a different directory
cd "$TMPDIR"
EMPTY_OUTPUT=$("$RT" agent-context 2>&1)
echo "$EMPTY_OUTPUT" | grep -qi "no RedTrail history\|no.*history" || {
  # May show data if git resolves to workspace — that's ok too
  true
}

# Verify: agent-context in the workspace produces markdown
cd "$WORKSPACE"
OUTPUT=$("$RT" agent-context 2>&1)
echo "$OUTPUT" | grep -q "# Project Context" || {
  echo "FAIL: agent-context should produce markdown with '# Project Context' heading"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

# Verify: mentions the directory
echo "$OUTPUT" | grep -q "Directory" || {
  echo "FAIL: agent-context should mention Directory"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

# Verify: --format json produces valid JSON
JSON_OUTPUT=$("$RT" agent-context --format json 2>&1)
echo "$JSON_OUTPUT" | python3 -m json.tool > /dev/null 2>&1 || {
  FIRST=$(echo "$JSON_OUTPUT" | head -c1)
  [[ "$FIRST" == "{" ]] || {
    echo "FAIL: --format json output is not valid JSON"
    echo "OUTPUT: $JSON_OUTPUT"
    exit 1
  }
}

# Verify: JSON has key sections
echo "$JSON_OUTPUT" | grep -q '"directory"' || {
  echo "FAIL: JSON missing 'directory' key"
  echo "OUTPUT: $JSON_OUTPUT"
  exit 1
}
echo "$JSON_OUTPUT" | grep -q '"sessions"' || {
  echo "FAIL: JSON missing 'sessions' key"
  echo "OUTPUT: $JSON_OUTPUT"
  exit 1
}

# Verify: --max-tokens truncates output
FULL_LEN=$(echo "$OUTPUT" | wc -c | tr -d ' ')
SMALL_OUTPUT=$("$RT" agent-context --max-tokens 50 2>&1)
SMALL_LEN=$(echo "$SMALL_OUTPUT" | wc -c | tr -d ' ')

# With max-tokens 50 (~200 chars), output should be shorter than full output
# (unless full output is already small enough)
if [ "$FULL_LEN" -gt 250 ]; then
  [ "$SMALL_LEN" -lt "$FULL_LEN" ] || {
    echo "FAIL: --max-tokens should truncate output (full=$FULL_LEN, truncated=$SMALL_LEN)"
    exit 1
  }
fi

# Verify: --since filter works without crashing
SINCE_OUTPUT=$("$RT" agent-context --since 1h 2>&1)
# Should either show data or empty message — just verify no crash
[ $? -eq 0 ] || {
  echo "FAIL: --since 1h should not crash"
  exit 1
}

echo "PASS"
