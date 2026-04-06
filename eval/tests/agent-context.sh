#!/usr/bin/env bash
# Live test: agent-context fast mode (--fast flag).
# Seeds multi-session agent data, verifies the redesigned output:
# - Last Session quick view
# - Session Activity with meaningful commands + icons
# - Git State section
# - Known Fixes (filtered: no retries, no tool errors, only project commands)
# - Unresolved Issues (only project command failures)
# - JSON format with meaningful_commands field
# - --since filtering
# - --max-tokens truncation
set -euo pipefail

RT="${RT_BIN:-/usr/local/bin/redtrail}"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"
export NO_COLOR=1

# Initialize a workspace with git
WORKSPACE="$TMPDIR/workspace"
mkdir -p "$WORKSPACE/src"
cd "$WORKSPACE"
git init -q
git config user.email "test@test.com"
git config user.name "Test"
echo "fn main() {}" > src/main.rs
git add -A && git commit -q -m "initial commit"

FAIL_COUNT=0
fail() {
  echo "FAIL: $1"
  FAIL_COUNT=$((FAIL_COUNT + 1))
}

# Helper: ingest a Claude Code tool event
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

# ═══════════════════════════════════════════════════════════════════════
# DATA SETUP
# ═══════════════════════════════════════════════════════════════════════

# ─── Session A: auth feature work ───
# Read → Edit (file modified), Write (file created)
# cargo test FAIL → Edit fix → cargo test PASS (resolved error)
ingest_event "Read" '{"file_path": "src/auth.rs"}' "session-A"
sleep 1
ingest_event "Edit" '{"file_path": "src/auth.rs", "old_string": "a", "new_string": "b"}' "session-A"
ingest_event "Write" '{"file_path": "src/middleware.rs", "content": "new"}' "session-A"
sleep 1
ingest_event "Bash" '{"command": "cargo test"}' "session-A" 'null' '"error: auth test failed"'
sleep 1
ingest_event "Edit" '{"file_path": "src/auth.rs", "old_string": "b", "new_string": "c"}' "session-A"
sleep 1
ingest_event "Bash" '{"command": "cargo test"}' "session-A" '{"stdout": "ok", "exitCode": 0}'

# ─── Session B: readme + build (no errors) ───
sleep 1
ingest_event "Read" '{"file_path": "README.md"}' "session-B"
ingest_event "Edit" '{"file_path": "README.md", "old_string": "old", "new_string": "new"}' "session-B"
ingest_event "Bash" '{"command": "cargo build"}' "session-B" '{"stdout": "ok", "exitCode": 0}'

# Sleep to age out sessions A+B
sleep 3

# ─── Session C: unresolved npm error ───
ingest_event "Bash" '{"command": "npm test"}' "session-C" 'null' '"Cannot find module express"'

# ═══════════════════════════════════════════════════════════════════════
# FAST MODE MARKDOWN ASSERTIONS
# ═══════════════════════════════════════════════════════════════════════
MD=$("$RT" agent-context --fast 2>"$TMPDIR/stderr_md.log")

# 1. Must have the main heading
echo "$MD" | grep -q "# Project Context (RedTrail)" || fail "missing main heading"

# 2. Must have Last Session section
echo "$MD" | grep -q "## Last Session" || fail "missing '## Last Session' section"

# 3. Must have Session Activity section
echo "$MD" | grep -q "## Session Activity" || fail "missing '## Session Activity' section"

# 4. Must have Git State section
echo "$MD" | grep -q "## Git State" || fail "missing '## Git State' section"

# 5. Git State must show current branch
echo "$MD" | sed -n '/## Git State/,/^## /p' | grep -q "Branch:" || \
  fail "Git State should show current branch"

# 6. Git State must show last commit
echo "$MD" | sed -n '/## Git State/,/^## /p' | grep -q "Last commit:" || \
  fail "Git State should show last commit"

# 7. Session Activity should contain status icons for meaningful commands
# At minimum we expect write/edit icons (~) and success icons (✓ or ✗)
echo "$MD" | sed -n '/## Session Activity/,/^## /p' | grep -qE "[~✓✗]" || \
  fail "Session Activity should show status icons (~ ✓ ✗)"

# 8. Session Activity should show Edit operations
echo "$MD" | sed -n '/## Session Activity/,/^## /p' | grep -q "Edit" || \
  fail "Session Activity should show Edit operations"

# 9. Session Activity should NOT show Read operations (those are noise)
if echo "$MD" | sed -n '/## Session Activity/,/^## /p' | grep -q "^. Read "; then
  fail "Session Activity should NOT show Read operations (filtered as noise)"
fi

# 10. Known Fixes should exist and contain the resolved cargo test error
echo "$MD" | grep -q "## Known Fixes" || fail "missing '## Known Fixes' section"
echo "$MD" | sed -n '/## Known Fixes/,/^## /p' | grep -q "cargo test" || \
  fail "Known Fixes should contain 'cargo test'"

# 11. Unresolved Issues should contain npm test
echo "$MD" | grep -q "## Unresolved Issues" || fail "missing '## Unresolved Issues' section"
echo "$MD" | sed -n '/## Unresolved Issues/,/^## /p' | grep -q "npm test" || \
  fail "Unresolved Issues should contain 'npm test'"

# 12. Unresolved Issues should NOT contain cargo test (that's resolved)
if echo "$MD" | sed -n '/## Unresolved Issues/,/^## /p' | grep -q "cargo test"; then
  fail "Unresolved Issues should NOT contain 'cargo test' (it was resolved)"
fi

# 13. Known Fixes should NOT contain npm test (that's unresolved)
if echo "$MD" | sed -n '/## Known Fixes/,/^## /p' | grep -q "npm test"; then
  fail "Known Fixes should NOT contain 'npm test' (that's unresolved)"
fi

# ═══════════════════════════════════════════════════════════════════════
# JSON FORMAT ASSERTIONS
# ═══════════════════════════════════════════════════════════════════════
JSON=$("$RT" agent-context --fast --format json 2>"$TMPDIR/stderr_json.log")

echo "$JSON" | python3 -m json.tool > /dev/null 2>&1 || {
  fail "--format json output is not valid JSON"
  echo "$JSON"
  if [ "$FAIL_COUNT" -gt 0 ]; then
    echo "FAILED: $FAIL_COUNT assertion(s) failed"
    exit 1
  fi
}

# 14. JSON should have git state fields
echo "$JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert 'branch' in d, f'JSON missing branch field'
assert 'uncommitted_count' in d, f'JSON missing uncommitted_count field'
assert 'last_commit_message' in d, f'JSON missing last_commit_message field'
assert 'sessions' in d, f'JSON missing sessions field'
" || fail "JSON structure assertions failed"

# 15. JSON sessions should have meaningful_commands
echo "$JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
for s in d['sessions']:
    assert 'meaningful_commands' in s, f'Session {s[\"session_id\"]} missing meaningful_commands'
" || fail "JSON sessions missing meaningful_commands"

# 16. JSON per-session counts
echo "$JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)

a = [s for s in d['sessions'] if s['session_id'] == 'session-A']
assert len(a) == 1, f'Expected 1 session-A, got {len(a)}'
a = a[0]
assert a['total_commands'] == 6, f'Session A total_commands={a[\"total_commands\"]}, expected 6'
assert a['errors_total'] >= 1, f'Session A errors_total={a[\"errors_total\"]}, expected >= 1'
assert a['errors_resolved'] >= 1, f'Session A errors_resolved={a[\"errors_resolved\"]}, expected >= 1'

b = [s for s in d['sessions'] if s['session_id'] == 'session-B']
assert len(b) == 1, f'Expected 1 session-B, got {len(b)}'
b = b[0]
assert b['total_commands'] == 3, f'Session B total_commands={b[\"total_commands\"]}, expected 3'
assert b['errors_total'] == 0, f'Session B errors_total={b[\"errors_total\"]}, expected 0'

c = [s for s in d['sessions'] if s['session_id'] == 'session-C']
assert len(c) == 1, f'Expected 1 session-C, got {len(c)}'
c = c[0]
assert c['total_commands'] == 1, f'Session C total_commands={c[\"total_commands\"]}, expected 1'
assert c['errors_total'] >= 1, f'Session C errors_total={c[\"errors_total\"]}, expected >= 1'
" || fail "Per-session JSON property assertions failed"

# ═══════════════════════════════════════════════════════════════════════
# --since FILTER TEST
# ═══════════════════════════════════════════════════════════════════════
SINCE_JSON=$("$RT" agent-context --fast --format json --since 2s 2>/dev/null)

echo "$SINCE_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
session_ids = [s['session_id'] for s in d['sessions']]

assert 'session-C' in session_ids, \
    f'--since 2s should include session-C, got: {session_ids}'
assert 'session-A' not in session_ids, \
    f'--since 2s should exclude session-A, got: {session_ids}'
assert 'session-B' not in session_ids, \
    f'--since 2s should exclude session-B, got: {session_ids}'
" || fail "--since filter assertions failed"

# ═══════════════════════════════════════════════════════════════════════
# --max-tokens TRUNCATION TEST
# ═══════════════════════════════════════════════════════════════════════
FULL_LEN=${#MD}

if [ "$FULL_LEN" -gt 300 ]; then
  SMALL=$("$RT" agent-context --fast --max-tokens 50 2>/dev/null)

  echo "$SMALL" | head -1 | grep -q "# Project Context" || \
    fail "Truncated output should start with '# Project Context'"

  echo "$SMALL" | grep -q "Truncated" || \
    fail "Truncated output should contain 'Truncated' notice"
else
  echo "WARN: full output too short ($FULL_LEN chars) to test truncation"
fi

# ═══════════════════════════════════════════════════════════════════════
# EMPTY PROJECT TEST
# ═══════════════════════════════════════════════════════════════════════
cd "$TMPDIR"
mkdir empty_project && cd empty_project
git init -q
EMPTY=$("$RT" agent-context --fast 2>/dev/null)
echo "$EMPTY" | grep -qi "no.*history" || {
  fail "Empty project should show 'no history' message, got: $EMPTY"
}

# ═══════════════════════════════════════════════════════════════════════
# RESULT
# ═══════════════════════════════════════════════════════════════════════
if [ "$FAIL_COUNT" -gt 0 ]; then
  echo ""
  echo "FAILED: $FAIL_COUNT assertion(s) failed"
  exit 1
fi

echo "PASS"
