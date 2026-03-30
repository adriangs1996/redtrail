#!/usr/bin/env bash
# Live test: seed agent events via ingest, then verify agent-report output
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
  local tool_response="${3:-null}"
  local error="${4:-null}"

  echo "{
    \"tool_name\": \"$tool_name\",
    \"tool_input\": $tool_input,
    \"tool_response\": $tool_response,
    \"error\": $error,
    \"cwd\": \"$WORKSPACE\",
    \"session_id\": \"test-agent-session-1\"
  }" | "$RT" ingest 2>/dev/null
}

# Simulate an agent session: read a file, edit it, run failing test, fix, run passing test
ingest_event "Read" '{"file_path": "src/lib.rs"}'
ingest_event "Edit" '{"file_path": "src/lib.rs", "old_string": "old", "new_string": "new"}'
ingest_event "Bash" '{"command": "cargo test"}' 'null' '"error: test auth::tests::test_login failed"'
ingest_event "Edit" '{"file_path": "src/lib.rs", "old_string": "broken", "new_string": "fixed"}'
ingest_event "Bash" '{"command": "cargo test"}' '{"stdout": "test result: ok. 5 passed", "exit_code": 0}'
ingest_event "Bash" '{"command": "git add -A"}' '{"stdout": "", "exit_code": 0}'
ingest_event "Bash" '{"command": "git commit -m fix"}' '{"stdout": "", "exit_code": 0}'

# Verify: agent-report default output works
OUTPUT=$("$RT" agent-report 2>&1)
echo "$OUTPUT" | grep -qi "report" || {
  echo "FAIL: agent-report output missing 'report' heading"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

# Verify: should mention files
echo "$OUTPUT" | grep -q "src/lib.rs" || {
  echo "FAIL: agent-report should mention src/lib.rs"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

# Verify: --json produces valid JSON
JSON_OUTPUT=$("$RT" agent-report --json 2>&1)
echo "$JSON_OUTPUT" | python3 -m json.tool > /dev/null 2>&1 || {
  # Fallback: check it starts with {
  FIRST=$(echo "$JSON_OUTPUT" | head -c1)
  [[ "$FIRST" == "{" ]] || {
    echo "FAIL: --json output is not valid JSON"
    echo "OUTPUT: $JSON_OUTPUT"
    exit 1
  }
}

# Verify: JSON has required nested structure
echo "$JSON_OUTPUT" | grep -q '"files"' || {
  echo "FAIL: JSON missing 'files' key"
  echo "OUTPUT: $JSON_OUTPUT"
  exit 1
}
echo "$JSON_OUTPUT" | grep -q '"commands"' || {
  echo "FAIL: JSON missing 'commands' key"
  echo "OUTPUT: $JSON_OUTPUT"
  exit 1
}
echo "$JSON_OUTPUT" | grep -q '"errors"' || {
  echo "FAIL: JSON missing 'errors' key"
  echo "OUTPUT: $JSON_OUTPUT"
  exit 1
}

# Verify: --markdown produces markdown
MD_OUTPUT=$("$RT" agent-report --markdown 2>&1)
echo "$MD_OUTPUT" | grep -q "^# " || {
  echo "FAIL: --markdown output missing markdown heading"
  echo "OUTPUT: $MD_OUTPUT"
  exit 1
}

# Verify: --session filter works
SESSION_OUTPUT=$("$RT" agent-report --session "test-agent-session-1" 2>&1)
echo "$SESSION_OUTPUT" | grep -qi "report" || {
  echo "FAIL: --session filter returned no data"
  echo "OUTPUT: $SESSION_OUTPUT"
  exit 1
}

# Verify: --session with nonexistent ID shows empty message
EMPTY_OUTPUT=$("$RT" agent-report --session "nonexistent-session" 2>&1)
echo "$EMPTY_OUTPUT" | grep -qi "no agent activity" || {
  echo "FAIL: nonexistent session should show 'no agent activity'"
  echo "OUTPUT: $EMPTY_OUTPUT"
  exit 1
}

echo "PASS"
