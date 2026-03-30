#!/usr/bin/env bash
# Live test: seed multi-session agent data, verify agent-context produces
# correct context document with session summaries, errors, workflow, and unresolved issues.
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
  local session_id="${3:-session-A}"
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

# ─── Session A: auth feature work ───
# Read, edit, test fail, fix, test pass
ingest_event "Read" '{"file_path": "src/auth.rs"}' "session-A"
ingest_event "Edit" '{"file_path": "src/auth.rs", "old_string": "a", "new_string": "b"}' "session-A"
ingest_event "Write" '{"file_path": "src/middleware.rs", "content": "new"}' "session-A"

sleep 1

ingest_event "Bash" '{"command": "cargo test"}' "session-A" 'null' '"error: auth test failed"'

sleep 1

ingest_event "Edit" '{"file_path": "src/auth.rs", "old_string": "b", "new_string": "c"}' "session-A"

sleep 1

ingest_event "Bash" '{"command": "cargo test"}' "session-A" '{"stdout": "ok", "exitCode": 0}'

sleep 1

# ─── Session B: readme update (simple, no errors) ───
ingest_event "Read" '{"file_path": "README.md"}' "session-B"
ingest_event "Edit" '{"file_path": "README.md", "old_string": "old", "new_string": "new"}' "session-B"
ingest_event "Bash" '{"command": "cargo build"}' "session-B" '{"stdout": "ok", "exitCode": 0}'

sleep 1

# ─── Session C: unresolved error ───
ingest_event "Bash" '{"command": "npm test"}' "session-C" 'null' '"Cannot find module express"'

# Total: 3 sessions, 10 events
# Session A: 6 commands, 1 error resolved, files: auth.rs modified, middleware.rs created
# Session B: 3 commands, 0 errors, files: README.md modified
# Session C: 1 command, 1 unresolved error

# ─── Test 1: Markdown output has all required sections ───
MD=$("$RT" agent-context 2>&1)

echo "$MD" | grep -q "^# Project Context" || {
  echo "FAIL: missing '# Project Context' heading"
  echo "$MD"
  exit 1
}

echo "$MD" | grep -q "^## Current State" || {
  echo "FAIL: missing '## Current State' section"
  echo "$MD"
  exit 1
}

echo "$MD" | grep -q "^## Recent Agent Sessions" || {
  echo "FAIL: missing '## Recent Agent Sessions' section"
  echo "$MD"
  exit 1
}

echo "$MD" | grep -q "^## Unresolved Issues" || {
  echo "FAIL: missing '## Unresolved Issues' section"
  echo "$MD"
  exit 1
}

# ─── Test 2: Current State includes directory ───
echo "$MD" | grep -q "$WORKSPACE" || {
  echo "FAIL: Current State should include the workspace directory"
  echo "$MD"
  exit 1
}

# ─── Test 3: Session summaries mention modified files ───
echo "$MD" | grep -q "src/auth.rs" || {
  echo "FAIL: sessions should mention src/auth.rs"
  echo "$MD"
  exit 1
}

echo "$MD" | grep -q "src/middleware.rs" || {
  echo "FAIL: sessions should mention src/middleware.rs (created in session A)"
  echo "$MD"
  exit 1
}

echo "$MD" | grep -q "README.md" || {
  echo "FAIL: sessions should mention README.md (modified in session B)"
  echo "$MD"
  exit 1
}

# ─── Test 4: Unresolved issues lists the npm test failure ───
echo "$MD" | grep -q "npm test" || {
  echo "FAIL: unresolved issues should mention 'npm test'"
  echo "$MD"
  exit 1
}

# ─── Test 5: Project Workflow section lists common commands ───
echo "$MD" | grep -q "^## Project Workflow" || {
  echo "FAIL: missing '## Project Workflow' section"
  echo "$MD"
  exit 1
}

# ─── Test 6: JSON output has correct structure and content ───
JSON=$("$RT" agent-context --format json 2>&1)

echo "$JSON" | python3 -m json.tool > /dev/null 2>&1 || {
  echo "FAIL: --format json output is not valid JSON"
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

# Directory should be set
check_json "d['directory'] is not None" "True"

# Should have 3 sessions (default takes 3 most recent)
check_json "len(d['sessions'])" "3"

# Session C is most recent (last ingested), has 1 command
# Find session-C in the list and verify
check_json "any(s['total_commands'] == 1 for s in d['sessions'])" "True"

# Session A should have 6 commands
check_json "any(s['total_commands'] == 6 for s in d['sessions'])" "True"

# Session B should have 3 commands
check_json "any(s['total_commands'] == 3 for s in d['sessions'])" "True"

# top_commands should include cargo
check_json "any('cargo' in c['command'] for c in d['top_commands'])" "True"

# unresolved_issues should have the npm test failure
check_json "len(d['unresolved_issues']) >= 1" "True"
check_json "any('npm' in u['failing_command'] for u in d['unresolved_issues'])" "True"

# ─── Test 7: --max-tokens truncates output ───
FULL_LEN=${#MD}

if [ "$FULL_LEN" -gt 300 ]; then
  SMALL=$("$RT" agent-context --max-tokens 50 2>&1)
  SMALL_LEN=${#SMALL}

  [ "$SMALL_LEN" -lt "$FULL_LEN" ] || {
    echo "FAIL: --max-tokens 50 should produce shorter output (full=$FULL_LEN, truncated=$SMALL_LEN)"
    exit 1
  }

  echo "$SMALL" | grep -q "Truncated" || {
    echo "FAIL: truncated output should contain 'Truncated' notice"
    echo "$SMALL"
    exit 1
  }
fi

# ─── Test 8: --since filter works ───
SINCE_OUTPUT=$("$RT" agent-context --since 1h 2>&1)
echo "$SINCE_OUTPUT" | grep -q "Project Context" || {
  echo "FAIL: --since 1h should still show data (all events are recent)"
  echo "$SINCE_OUTPUT"
  exit 1
}

# ─── Test 9: Empty project shows no-history message ───
cd "$TMPDIR"
mkdir empty_project && cd empty_project
git init -q
EMPTY=$("$RT" agent-context 2>&1)
echo "$EMPTY" | grep -qi "no.*history" || {
  echo "FAIL: empty project should show 'no history' message"
  echo "$EMPTY"
  exit 1
}

echo "PASS"
