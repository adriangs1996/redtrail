#!/usr/bin/env bash
# Live test: seed error/fix sequences, then verify resolve finds them
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
    \"session_id\": \"resolve-test-session\"
  }" | "$RT" ingest 2>/dev/null
}

# Seed a repeating error pattern: "Cannot find module bcrypt" -> npm install fixes it
# Occurrence 1
ingest_event "Bash" '{"command": "npm test"}' 'null' '"Cannot find module bcrypt"'
ingest_event "Bash" '{"command": "npm install bcrypt"}' '{"stdout": "added 1 package", "exit_code": 0}'
ingest_event "Bash" '{"command": "npm test"}' '{"stdout": "Tests passed", "exit_code": 0}'

# Occurrence 2 (same error, same fix — builds frequency)
ingest_event "Bash" '{"command": "npm test"}' 'null' '"Cannot find module bcrypt"'
ingest_event "Bash" '{"command": "npm install bcrypt"}' '{"stdout": "added 1 package", "exit_code": 0}'
ingest_event "Bash" '{"command": "npm test"}' '{"stdout": "Tests passed", "exit_code": 0}'

# Seed a different error
ingest_event "Bash" '{"command": "cargo build"}' 'null' '"error[E0308]: mismatched types"'

# Verify: resolve finds the bcrypt error
OUTPUT=$("$RT" resolve "Cannot find module" 2>&1)
echo "$OUTPUT" | grep -qi "found\|matching" || {
  echo "FAIL: resolve should find matching errors"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

# Verify: output mentions bcrypt or the error
echo "$OUTPUT" | grep -qi "bcrypt\|module\|find" || {
  echo "FAIL: resolve output should reference the error pattern"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

# Verify: resolve with no matches shows helpful message
NO_MATCH=$("$RT" resolve "totally unique error that never happened xyz123" 2>&1)
echo "$NO_MATCH" | grep -qi "no matching" || {
  echo "FAIL: unmatched error should show 'no matching' message"
  echo "OUTPUT: $NO_MATCH"
  exit 1
}

# Verify: --json produces valid JSON when there are results
JSON_OUTPUT=$("$RT" resolve "Cannot find module" --json 2>&1)
echo "$JSON_OUTPUT" | python3 -m json.tool > /dev/null 2>&1 || {
  FIRST=$(echo "$JSON_OUTPUT" | head -c1)
  [[ "$FIRST" == "{" ]] || {
    echo "FAIL: --json output is not valid JSON"
    echo "OUTPUT: $JSON_OUTPUT"
    exit 1
  }
}

# Verify: JSON has required fields
echo "$JSON_OUTPUT" | grep -q '"query"' || {
  echo "FAIL: JSON missing 'query' key"
  echo "OUTPUT: $JSON_OUTPUT"
  exit 1
}
echo "$JSON_OUTPUT" | grep -q '"resolutions"' || {
  echo "FAIL: JSON missing 'resolutions' key"
  echo "OUTPUT: $JSON_OUTPUT"
  exit 1
}

# Verify: --global flag doesn't crash
GLOBAL_OUTPUT=$("$RT" resolve "error" --global 2>&1)
[ $? -eq 0 ] || {
  echo "FAIL: --global flag should not crash"
  exit 1
}

# Verify: --cmd filter doesn't crash
CMD_OUTPUT=$("$RT" resolve "error" --cmd npm 2>&1)
[ $? -eq 0 ] || {
  echo "FAIL: --cmd filter should not crash"
  exit 1
}

# Verify: short error message is rejected
SHORT_OUTPUT=$("$RT" resolve "err" 2>&1)
echo "$SHORT_OUTPUT" | grep -qi "short\|specific\|too" || {
  echo "FAIL: very short error message should be rejected"
  echo "OUTPUT: $SHORT_OUTPUT"
  exit 1
}

# Verify: piped input via --stdin works
STDIN_OUTPUT=$(echo "Cannot find module bcrypt" | "$RT" resolve --stdin 2>&1)
echo "$STDIN_OUTPUT" | grep -qi "found\|matching\|bcrypt" || {
  echo "FAIL: --stdin should find the same errors"
  echo "OUTPUT: $STDIN_OUTPUT"
  exit 1
}

echo "PASS"
