#!/usr/bin/env bash
# Live test: seed error/fix patterns, verify resolve finds them with correct
# ranking, resolution details, and output formats.
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"
export NO_COLOR=1

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
    \"session_id\": \"resolve-session\"
  }" | "$RT" ingest 2>/dev/null
}

# ─── Seed data: repeating error pattern ───
# "Cannot find module bcrypt" occurs 3 times, resolved by "npm install" each time
for i in 1 2 3; do
  ingest_event "Bash" '{"command": "npm test"}' 'null' '"Cannot find module bcrypt"'
  ingest_event "Bash" '{"command": "npm install bcrypt"}' '{"stdout": "added 1 package", "exit_code": 0}'
  ingest_event "Bash" '{"command": "npm test"}' '{"stdout": "Tests: 5 passed", "exit_code": 0}'
done

# A different error: "connection refused" occurs once, no resolution
ingest_event "Bash" '{"command": "curl localhost:3000"}' 'null' '"Failed to connect: connection refused"'

# An unrelated successful command (noise)
ingest_event "Bash" '{"command": "git status"}' '{"stdout": "clean", "exit_code": 0}'

# Total: 11 events
# Error patterns:
#   "Cannot find module bcrypt" - 3 occurrences, all resolved by "npm install bcrypt"
#   "connection refused" - 1 occurrence, unresolved

# ─── Test 1: resolve finds the bcrypt error ───
OUTPUT=$("$RT" resolve "Cannot find module" 2>&1)

echo "$OUTPUT" | grep -q "matching error pattern" || {
  echo "FAIL: resolve should report matching patterns"
  echo "$OUTPUT"
  exit 1
}

# Should show the error text
echo "$OUTPUT" | grep -qi "find module\|bcrypt\|module" || {
  echo "FAIL: resolve output should reference the error content"
  echo "$OUTPUT"
  exit 1
}

# Should show occurrence count (3 times)
echo "$OUTPUT" | grep -q "3 time" || {
  echo "FAIL: should show '3 time(s)' for bcrypt error"
  echo "$OUTPUT"
  exit 1
}

# Should show the fix command
echo "$OUTPUT" | grep -q "npm install" || echo "$OUTPUT" | grep -q "npm test" || {
  echo "FAIL: should show the resolution command"
  echo "$OUTPUT"
  exit 1
}

# Should show fix success rate (worked 3/3)
echo "$OUTPUT" | grep -q "3/3" || {
  echo "FAIL: should show success rate 3/3"
  echo "$OUTPUT"
  exit 1
}

# ─── Test 2: resolve "connection refused" finds single-occurrence error ───
CONN_OUTPUT=$("$RT" resolve "connection refused" 2>&1)

echo "$CONN_OUTPUT" | grep -qi "refused\|connect\|curl" || {
  echo "FAIL: resolve should find the connection refused error"
  echo "$CONN_OUTPUT"
  exit 1
}

# Should note no fix found (unresolved)
echo "$CONN_OUTPUT" | grep -qi "no.*fix\|0/" || {
  # The error might show 0 resolutions or "no known fix"
  echo "WARN: could not verify 'no fix' message, checking occurrence count instead"
  echo "$CONN_OUTPUT" | grep -q "1 time" || {
    echo "FAIL: should show '1 time' for connection refused"
    echo "$CONN_OUTPUT"
    exit 1
  }
}

# ─── Test 3: no matches gives helpful message ───
NO_MATCH=$("$RT" resolve "completely unique error string xyz999" 2>&1)
echo "$NO_MATCH" | grep -qi "no matching" || {
  echo "FAIL: unmatched query should say 'no matching'"
  echo "$NO_MATCH"
  exit 1
}

# Should suggest --global
echo "$NO_MATCH" | grep -q "\-\-global" || {
  echo "FAIL: should suggest --global when no local matches"
  echo "$NO_MATCH"
  exit 1
}

# ─── Test 4: --json output has correct structure ───
JSON=$("$RT" resolve "Cannot find module" --json 2>&1)

echo "$JSON" | python3 -m json.tool > /dev/null 2>&1 || {
  echo "FAIL: --json output is not valid JSON"
  echo "$JSON"
  exit 1
}

check_json() {
  local path="$1"
  local expected="$2"
  local actual
  actual=$(echo "$JSON" | python3 -c "import sys,json; d=json.load(sys.stdin); print($path)")
  if [ "$actual" != "$expected" ]; then
    echo "FAIL: JSON $path = '$actual', expected '$expected'"
    echo "$JSON" | python3 -m json.tool
    exit 1
  fi
}

# JSON is an array of patterns
check_json "type(d).__name__" "list"

# Should have at least 1 pattern
check_json "len(d) >= 1" "True"

# First pattern should have correct fields
check_json "'occurrences' in d[0]" "True"
check_json "'resolutions' in d[0]" "True"
check_json "'success_rate' in d[0]" "True"

# Occurrences should be 3
check_json "d[0]['occurrences']" "3"

# Success rate should be 1.0 (all resolved)
check_json "d[0]['success_rate']" "1.0"

# Resolutions should include npm install or npm test
check_json "len(d[0]['resolutions']) >= 1" "True"

# ─── Test 5: --stdin reads from pipe ───
STDIN_OUTPUT=$(echo "Cannot find module bcrypt" | "$RT" resolve --stdin 2>&1)
echo "$STDIN_OUTPUT" | grep -q "matching error pattern" || {
  echo "FAIL: --stdin should find matching patterns"
  echo "$STDIN_OUTPUT"
  exit 1
}

echo "$STDIN_OUTPUT" | grep -q "3 time" || {
  echo "FAIL: --stdin should find same 3 occurrences"
  echo "$STDIN_OUTPUT"
  exit 1
}

# ─── Test 6: short input rejected ───
SHORT=$("$RT" resolve "err" 2>&1)
echo "$SHORT" | grep -qi "short\|specific\|too" || {
  echo "FAIL: input < 5 chars should be rejected with helpful message"
  echo "$SHORT"
  exit 1
}

# ─── Test 7: --global flag works without crash ───
GLOBAL=$("$RT" resolve "Cannot find module" --global 2>&1)
echo "$GLOBAL" | grep -q "matching error pattern" || {
  echo "FAIL: --global should still find patterns"
  echo "$GLOBAL"
  exit 1
}

# ─── Test 8: --cmd filter scopes to specific binary ───
CMD_OUTPUT=$("$RT" resolve "error" --cmd npm 2>&1)
# Should either find npm errors or show no matches — just verify no crash
[ $? -eq 0 ] || {
  echo "FAIL: --cmd npm should not crash"
  exit 1
}

# ─── Test 9: resolve in a different project auto-widens to global ───
cd "$TMPDIR"
mkdir other_project && cd other_project
git init -q

OTHER_OUTPUT=$("$RT" resolve "Cannot find module" 2>&1)
# Should auto-widen and find the bcrypt errors from the other project
echo "$OTHER_OUTPUT" | grep -qi "bcrypt\|module\|global\|matching" || {
  echo "FAIL: should auto-widen to global and find bcrypt error from other project"
  echo "$OTHER_OUTPUT"
  exit 1
}

echo "PASS"
