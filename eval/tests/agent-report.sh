#!/usr/bin/env bash
# Live test: seed agent events via ingest, verify agent-report produces
# correct analysis with accurate counts, file lists, error sequences, and test results.
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
# Note: Claude Code uses "exitCode" (camelCase) in tool_response
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

# ─── Seed a realistic agent session ───
# Events must be separated by sleeps so timestamps differ (ingest uses now()).
# Group events that can share a timestamp, sleep between ordering-critical points.

# Phase 1: Investigation (reads)
ingest_event "Read" '{"file_path": "src/auth.rs"}'
sleep 1
ingest_event "Read" '{"file_path": "src/config.rs"}'
sleep 1

# Phase 2: First edit + new file
ingest_event "Edit" '{"file_path": "src/auth.rs", "old_string": "old", "new_string": "new"}'
sleep 1
ingest_event "Write" '{"file_path": "src/middleware.rs", "content": "pub fn auth() {}"}'
sleep 1

# Phase 3: Test FAILS
ingest_event "Bash" '{"command": "cargo test"}' 'null' '"error: test auth::tests::test_login failed"'
sleep 1

# Phase 4: Fix + test PASSES
ingest_event "Edit" '{"file_path": "src/auth.rs", "old_string": "bug", "new_string": "fix"}'

sleep 1

ingest_event "Bash" '{"command": "cargo test"}' '{"stdout": "test result: ok. 5 passed; 0 failed", "exitCode": 0}'
sleep 1

# Phase 5: Git operations
ingest_event "Bash" '{"command": "git add -A"}' '{"stdout": "", "exitCode": 0}'
sleep 1
ingest_event "Bash" '{"command": "git commit -m \"fix auth\""}' '{"stdout": "1 file changed", "exitCode": 0}'

# Total: 9 events
# Files created: src/middleware.rs (Write, never Read before)
# Files modified: src/auth.rs (Read then Edit)
# Files read only: src/config.rs (Read, never written)
# Errors: 1 (cargo test failed then succeeded = resolved)
# Test runs: 2 (1 failed, 1 passed)
# Git operations: 2 (add, commit)

# ─── Test 1: JSON output has correct counts and structure ───
JSON=$("$RT" agent-report --json 2>&1)

# Validate JSON is parseable
echo "$JSON" | python3 -m json.tool >/dev/null 2>&1 || {
  echo "FAIL: --json output is not valid JSON"
  echo "$JSON"
  exit 1
}

# Extract values via python3
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

# Source should be claude_code
check_json "d['source']" "claude_code"

# Total commands: 9
check_json "d['commands']['total']" "9"

# All 9 commands are agent (source=claude_code), 0 human
check_json "d['commands']['agent']" "9"
check_json "d['commands']['human']" "0"

# Files created: should contain src/middleware.rs
check_json "'src/middleware.rs' in d['files']['created']" "True"

# Files modified: should contain src/auth.rs
check_json "'src/auth.rs' in d['files']['modified']" "True"

# Files read only: should contain src/config.rs
check_json "'src/config.rs' in d['files']['read_only']" "True"

# File counts
check_json "len(d['files']['created'])" "1"
check_json "len(d['files']['modified'])" "1"
check_json "len(d['files']['read_only'])" "1"

# Test runs: 2 total, 1 passed, 1 failed
check_json "d['tests']['total_runs']" "2"
check_json "d['tests']['passed']" "1"
check_json "d['tests']['failed']" "1"

# Errors: should have at least 1 error sequence
check_json "len(d['errors']) >= 1" "True"

# First error should be resolved
check_json "d['errors'][0]['resolved']" "True"

# by_binary should have cargo and git entries
check_json "'cargo' in d['commands']['by_binary']" "True"
check_json "'git' in d['commands']['by_binary']" "True"

# cargo stats: 2 total (1 fail + 1 pass)
check_json "d['commands']['by_binary']['cargo']['total']" "2"
check_json "d['commands']['by_binary']['cargo']['failed']" "1"
check_json "d['commands']['by_binary']['cargo']['succeeded']" "1"

# git stats: 2 total, both succeeded
check_json "d['commands']['by_binary']['git']['total']" "2"
check_json "d['commands']['by_binary']['git']['succeeded']" "2"
check_json "d['commands']['by_binary']['git']['failed']" "0"

# ─── Test 2: ASCII output contains the right sections ───
ASCII=$("$RT" agent-report 2>&1)

# Header with source and command count
echo "$ASCII" | grep -q "claude_code" || {
  echo "FAIL: ASCII missing source 'claude_code'"
  echo "$ASCII"
  exit 1
}

# Files section with correct prefixes
echo "$ASCII" | grep -q "+ src/middleware.rs" || {
  echo "FAIL: ASCII missing '+ src/middleware.rs' (created)"
  echo "$ASCII"
  exit 1
}
echo "$ASCII" | grep -q "~ src/auth.rs" || {
  echo "FAIL: ASCII missing '~ src/auth.rs' (modified)"
  echo "$ASCII"
  exit 1
}
echo "$ASCII" | grep -q "src/config.rs" || {
  echo "FAIL: ASCII missing 'src/config.rs' (read only)"
  echo "$ASCII"
  exit 1
}

# Error sequences section with resolved status
echo "$ASCII" | grep -q "Error Sequences" || {
  echo "FAIL: ASCII missing 'Error Sequences' section"
  echo "$ASCII"
  exit 1
}
echo "$ASCII" | grep -q "resolved" || {
  echo "FAIL: ASCII missing 'resolved' status"
  echo "$ASCII"
  exit 1
}

# Tests section
echo "$ASCII" | grep -q "Tests" || {
  echo "FAIL: ASCII missing 'Tests' section"
  echo "$ASCII"
  exit 1
}

# Summary section
echo "$ASCII" | grep -q "Summary" || {
  echo "FAIL: ASCII missing 'Summary' section"
  echo "$ASCII"
  exit 1
}

# ─── Test 3: Markdown output has correct structure ───
MD=$("$RT" agent-report --markdown 2>&1)

echo "$MD" | grep -q "^# Agent Report" || {
  echo "FAIL: Markdown missing '# Agent Report' heading"
  echo "$MD"
  exit 1
}
echo "$MD" | grep -q "src/middleware.rs" || {
  echo "FAIL: Markdown missing src/middleware.rs"
  echo "$MD"
  exit 1
}
echo "$MD" | grep -q "src/auth.rs" || {
  echo "FAIL: Markdown missing src/auth.rs"
  echo "$MD"
  exit 1
}

# ─── Test 4: --session filter returns correct data ───
SESSION_JSON=$("$RT" agent-report --session "test-agent-session-1" --json 2>&1)
SESSION_COUNT=$(echo "$SESSION_JSON" | python3 -c "import sys,json; print(json.load(sys.stdin)['commands']['total'])")
[ "$SESSION_COUNT" -eq 9 ] || {
  echo "FAIL: --session should return all 9 commands, got $SESSION_COUNT"
  exit 1
}

# ─── Test 5: nonexistent session shows empty message ───
EMPTY=$("$RT" agent-report --session "nonexistent-session-id" 2>&1)
echo "$EMPTY" | grep -qi "no agent activity" || {
  echo "FAIL: nonexistent session should say 'no agent activity'"
  echo "$EMPTY"
  exit 1
}

echo "PASS"
