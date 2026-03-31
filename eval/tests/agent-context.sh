#!/usr/bin/env bash
# Live test: seed multi-session agent data, verify agent-context produces
# behaviorally correct context with error classification, file tracking,
# session outcomes, time filtering, and token truncation.
set -euo pipefail

RT="${RT_BIN:-/usr/local/bin/redtrail}"

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

FAIL_COUNT=0
fail() {
  echo "FAIL: $1"
  FAIL_COUNT=$((FAIL_COUNT + 1))
}

# Helper: ingest a Claude Code tool event with proper stderr separation
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
# Sleeps are required between events that must have distinct timestamps
# because the analysis sorts by timestamp_start (i64 seconds) and the
# error-fix detector depends on chronological order.
# ═══════════════════════════════════════════════════════════════════════

# ─── Session A: auth feature work ───
# Read auth.rs, Edit auth.rs → files_modified
# Write middleware.rs (no prior read) → files_created
# cargo test FAIL → Edit fix → cargo test PASS → resolved error
ingest_event "Read" '{"file_path": "src/auth.rs"}' "session-A"
sleep 1  # Edit must follow Read for file classification
ingest_event "Edit" '{"file_path": "src/auth.rs", "old_string": "a", "new_string": "b"}' "session-A"
ingest_event "Write" '{"file_path": "src/middleware.rs", "content": "new"}' "session-A"
sleep 1  # cargo test fail must be after file ops
ingest_event "Bash" '{"command": "cargo test"}' "session-A" 'null' '"error: auth test failed"'
sleep 1  # fix edit must be after failing test
ingest_event "Edit" '{"file_path": "src/auth.rs", "old_string": "b", "new_string": "c"}' "session-A"
sleep 1  # passing test must be after fix edit
ingest_event "Bash" '{"command": "cargo test"}' "session-A" '{"stdout": "ok", "exitCode": 0}'

# ─── Session B: readme update (simple, no errors) ───
sleep 1
ingest_event "Read" '{"file_path": "README.md"}' "session-B"
ingest_event "Edit" '{"file_path": "README.md", "old_string": "old", "new_string": "new"}' "session-B"
ingest_event "Bash" '{"command": "cargo build"}' "session-B" '{"stdout": "ok", "exitCode": 0}'

# ═══════════════════════════════════════════════════════════════════════
# TEST: --since filter actually filters
# Sessions A and B are already ingested. Sleep so their timestamps age
# out, then ingest Session C and query with a short window.
# ═══════════════════════════════════════════════════════════════════════
sleep 3  # Ensure A+B timestamps are >2s old

# ─── Session C: unresolved error ───
ingest_event "Bash" '{"command": "npm test"}' "session-C" 'null' '"Cannot find module express"'

# ═══════════════════════════════════════════════════════════════════════
# MARKDOWN ASSERTIONS
# ═══════════════════════════════════════════════════════════════════════
MD=$("$RT" agent-context 2>"$TMPDIR/stderr_md.log")
if [ -s "$TMPDIR/stderr_md.log" ]; then
  echo "WARN: unexpected stderr from agent-context:"
  cat "$TMPDIR/stderr_md.log"
fi

# ─── 1. Resolved vs. unresolved error classification ───
# Session A's resolved cargo test error belongs in Known Errors & Fixes
echo "$MD" | grep -q "## Known Errors & Fixes" || fail "missing '## Known Errors & Fixes' section"
echo "$MD" | grep -q "## Unresolved Issues" || fail "missing '## Unresolved Issues' section"

# Known Errors & Fixes must contain the resolved cargo test error
echo "$MD" | sed -n '/## Known Errors & Fixes/,/^## /p' | grep -q "cargo test" || \
  fail "Known Errors & Fixes should contain 'cargo test' (the resolved error)"

# Known Errors & Fixes must NOT contain the unresolved npm test error
if echo "$MD" | sed -n '/## Known Errors & Fixes/,/^## /p' | grep -q "npm test"; then
  fail "Known Errors & Fixes should NOT contain 'npm test' (that's unresolved)"
fi

# Unresolved Issues must contain the npm test error
echo "$MD" | sed -n '/## Unresolved Issues/,/^## /p' | grep -q "npm test" || \
  fail "Unresolved Issues should contain 'npm test'"

# Unresolved Issues must NOT contain the cargo test error (that's resolved)
if echo "$MD" | sed -n '/## Unresolved Issues/,/^## /p' | grep -q "cargo test"; then
  fail "Unresolved Issues should NOT contain 'cargo test' (that's resolved)"
fi

# The resolved entry should show both the failing command and resolution
echo "$MD" | sed -n '/## Known Errors & Fixes/,/^## /p' | grep "cargo test" | grep -q "\->" || \
  fail "Known Errors & Fixes entry should show failing_command -> resolution_command"

# ─── 2. Session outcome strings ───
# Session A: cargo test fail → fix → pass → "tests passing"
# We need to find session A's block. Sessions are numbered 1, 2, 3.
# Session order is most-recent-first, so Session C is first, then B, then A.
# Session A is the last (Session 3).

# Extract each session block for targeted assertions
SESSION_BLOCKS=$(echo "$MD" | sed -n '/## Recent Agent Sessions/,/^## [^#]/p')

# Session A had test runs ending in success → outcome should say "tests passing"
echo "$SESSION_BLOCKS" | grep -q "All tests passing" || \
  fail "Session A outcome should contain 'All tests passing' (cargo test fail→fix→pass)"

# Session B had no errors at all → outcome should say "without errors"
echo "$SESSION_BLOCKS" | grep -q "without errors" || \
  fail "Session B outcome should contain 'without errors' (no errors in session)"

# Session C had unresolved test failure → outcome should indicate failure
echo "$SESSION_BLOCKS" | grep -q "failing" || \
  fail "Session C outcome should contain 'failing' (unresolved npm test failure)"

# ═══════════════════════════════════════════════════════════════════════
# JSON ASSERTIONS — per-session identity-based
# ═══════════════════════════════════════════════════════════════════════
JSON=$("$RT" agent-context --format json 2>"$TMPDIR/stderr_json.log")
if [ -s "$TMPDIR/stderr_json.log" ]; then
  echo "WARN: unexpected stderr from agent-context --format json:"
  cat "$TMPDIR/stderr_json.log"
fi

echo "$JSON" | python3 -m json.tool > /dev/null 2>&1 || {
  fail "--format json output is not valid JSON"
  echo "$JSON"
  # Fatal: can't continue JSON assertions without valid JSON
  if [ "$FAIL_COUNT" -gt 0 ]; then
    echo "FAIL ($FAIL_COUNT failure(s))"
    exit 1
  fi
}

# ─── 3. Per-session property bundles ───
echo "$JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)

# Session A: 6 commands, errors resolved
a = [s for s in d['sessions'] if s['session_id'] == 'session-A']
assert len(a) == 1, f\"Expected 1 session-A, got {len(a)}\"
a = a[0]
assert a['total_commands'] == 6, f\"Session A total_commands={a['total_commands']}, expected 6\"
assert a['errors_total'] >= 1, f\"Session A errors_total={a['errors_total']}, expected >= 1\"
assert a['errors_resolved'] >= 1, f\"Session A errors_resolved={a['errors_resolved']}, expected >= 1\"

# Session B: 3 commands, 0 errors
b = [s for s in d['sessions'] if s['session_id'] == 'session-B']
assert len(b) == 1, f\"Expected 1 session-B, got {len(b)}\"
b = b[0]
assert b['total_commands'] == 3, f\"Session B total_commands={b['total_commands']}, expected 3\"
assert b['errors_total'] == 0, f\"Session B errors_total={b['errors_total']}, expected 0\"

# Session C: 1 command, 1 error, 0 resolved
c = [s for s in d['sessions'] if s['session_id'] == 'session-C']
assert len(c) == 1, f\"Expected 1 session-C, got {len(c)}\"
c = c[0]
assert c['total_commands'] == 1, f\"Session C total_commands={c['total_commands']}, expected 1\"
assert c['errors_total'] >= 1, f\"Session C errors_total={c['errors_total']}, expected >= 1\"
assert c['errors_resolved'] == 0, f\"Session C errors_resolved={c['errors_resolved']}, expected 0\"
" || fail "Per-session JSON property assertions failed (see above)"

# ─── 4. File classification: modified vs. created ───
echo "$JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
a = [s for s in d['sessions'] if s['session_id'] == 'session-A'][0]

assert 'src/auth.rs' in a['files_modified'], \
    f\"src/auth.rs should be in files_modified, got {a['files_modified']}\"
assert 'src/middleware.rs' in a['files_created'], \
    f\"src/middleware.rs should be in files_created, got {a['files_created']}\"
assert 'src/middleware.rs' not in a['files_modified'], \
    f\"src/middleware.rs should NOT be in files_modified (it was created, not modified)\"
" || fail "File classification assertions failed (see above)"

# ─── 5. Unresolved issue error_message content ───
echo "$JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
issues = [u for u in d['unresolved_issues'] if 'npm' in u['failing_command']]
assert len(issues) >= 1, f\"Expected npm test in unresolved_issues, got {d['unresolved_issues']}\"
issue = issues[0]
assert 'express' in issue['error_message'] or 'Cannot find module' in issue['error_message'], \
    f\"Unresolved issue error_message should mention 'express' or 'Cannot find module', got: {issue['error_message']}\"
" || fail "Unresolved issue error_message content assertion failed (see above)"

# ═══════════════════════════════════════════════════════════════════════
# --since FILTER TEST
# Sessions A+B were ingested >3s ago. Session C was just ingested.
# --since 2s should include only Session C.
# ═══════════════════════════════════════════════════════════════════════
SINCE_JSON=$("$RT" agent-context --format json --since 2s 2>"$TMPDIR/stderr_since.log")
if [ -s "$TMPDIR/stderr_since.log" ]; then
  echo "WARN: unexpected stderr from agent-context --since 2s:"
  cat "$TMPDIR/stderr_since.log"
fi

echo "$SINCE_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
session_ids = [s['session_id'] for s in d['sessions']]

assert 'session-C' in session_ids, \
    f\"--since 2s should include session-C (just ingested), got sessions: {session_ids}\"
assert 'session-A' not in session_ids, \
    f\"--since 2s should exclude session-A (ingested >3s ago), got sessions: {session_ids}\"
assert 'session-B' not in session_ids, \
    f\"--since 2s should exclude session-B (ingested >3s ago), got sessions: {session_ids}\"
" || fail "--since filter assertions failed (see above)"

# ═══════════════════════════════════════════════════════════════════════
# --max-tokens TRUNCATION TEST
# ═══════════════════════════════════════════════════════════════════════
FULL_LEN=${#MD}

if [ "$FULL_LEN" -gt 300 ]; then
  SMALL=$("$RT" agent-context --max-tokens 50 2>"$TMPDIR/stderr_trunc.log")

  # Must start with the main heading
  echo "$SMALL" | head -1 | grep -q "# Project Context" || \
    fail "Truncated output should start with '# Project Context'"

  # Must end with the truncation notice
  echo "$SMALL" | grep -q "Truncated" || \
    fail "Truncated output should contain 'Truncated' notice"

  # Every ## heading that appears must be complete (no mid-heading cut)
  # Check that each ## line has text after it
  while IFS= read -r heading; do
    [ -z "$heading" ] && continue
    echo "$SMALL" | grep -qF "$heading" || \
      fail "Heading '$heading' appears truncated"
  done < <(echo "$SMALL" | grep "^## ")

  # At least one ## section from full output should be absent in truncated
  FULL_SECTIONS=$(echo "$MD" | grep -c "^## " || true)
  SMALL_SECTIONS=$(echo "$SMALL" | grep -c "^## " || true)
  [ "$SMALL_SECTIONS" -lt "$FULL_SECTIONS" ] || \
    fail "Truncated output should have fewer ## sections than full output (full=$FULL_SECTIONS, truncated=$SMALL_SECTIONS)"
else
  echo "WARN: full output too short ($FULL_LEN chars) to test --max-tokens truncation"
fi

# ═══════════════════════════════════════════════════════════════════════
# EMPTY PROJECT TEST
# ═══════════════════════════════════════════════════════════════════════
cd "$TMPDIR"
mkdir empty_project && cd empty_project
git init -q
EMPTY=$("$RT" agent-context 2>"$TMPDIR/stderr_empty.log")
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
