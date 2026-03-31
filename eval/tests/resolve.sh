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
    \"session_id\": \"resolve-session\"
  }" | "$RT" ingest 2>/dev/null
}

# ─── Seed data: repeating error pattern ───
# "Cannot find module bcrypt" occurs 3 times, each resolved by "npm install" then "npm test" pass
# Must sleep between error and resolution so timestamps differ.

for i in 1 2 3; do
  # Error
  ingest_event "Bash" '{"command": "npm test"}' 'null' '"Cannot find module bcrypt"'
  sleep 1
  # Fix
  ingest_event "Bash" '{"command": "npm install bcrypt"}' '{"stdout": "added 1 package", "exitCode": 0}'
  # Resolution (same binary succeeds)
  ingest_event "Bash" '{"command": "npm test"}' '{"stdout": "Tests: 5 passed", "exitCode": 0}'
  sleep 1
done

# A different error: "connection refused" occurs once, no resolution
ingest_event "Bash" '{"command": "curl localhost:3000"}' 'null' '"Failed to connect: connection refused"'
sleep 1

# An unrelated successful command (noise)
ingest_event "Bash" '{"command": "git status"}' '{"stdout": "clean", "exitCode": 0}'

# Unresolved error for ranking test: shares "Cannot find module" prefix but no fix
ingest_event "Bash" '{"command": "npm test"}' 'null' '"Cannot find module express"'
sleep 1

# Error with file path + line number for normalization test
ingest_event "Bash" '{"command": "node app.js"}' 'null' '"Error: Cannot read file /home/user/project/src/index.js:42:10"'
sleep 1

# Total: 14 events
# Error patterns:
#   "Cannot find module bcrypt" - 3 occurrences, all resolved
#   "connection refused" - 1 occurrence, unresolved
#   "Cannot find module express" - 1 occurrence, unresolved
#   "Cannot read file ..." - 1 occurrence, unresolved

# ─── Test 1: resolve finds the bcrypt error with correct counts ───
OUTPUT=$("$RT" resolve "Cannot find module" 2>&1)

echo "$OUTPUT" | grep -q "matching error pattern" || {
  echo "FAIL: resolve should report matching patterns"
  echo "$OUTPUT"
  exit 1
}

# Should show occurrence count (3 times)
echo "$OUTPUT" | grep -q "3 time" || {
  echo "FAIL: should show '3 time(s)' for bcrypt error"
  echo "$OUTPUT"
  exit 1
}

# Should show the fix command — find_resolution picks the first successful
# same-binary command after the failure, which is "npm install bcrypt"
echo "$OUTPUT" | grep -q "npm install bcrypt" || {
  echo "FAIL: should show 'npm install bcrypt' as the resolution command"
  echo "$OUTPUT"
  exit 1
}

# Should show fix success rate in "worked 3/3" format
echo "$OUTPUT" | grep -q "3/3" || {
  echo "FAIL: should show success rate as '3/3'"
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

# Should show 1 occurrence
echo "$CONN_OUTPUT" | grep -q "1 time" || {
  echo "FAIL: should show '1 time' for connection refused"
  echo "$CONN_OUTPUT"
  exit 1
}

# Should show no resolution found (this error has no fix)
echo "$CONN_OUTPUT" | grep -q "No known fix found" || {
  echo "FAIL: connection refused error should show 'No known fix found'"
  echo "$CONN_OUTPUT"
  exit 1
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
JSON=$("$RT" resolve "Cannot find module" --json 2>/dev/null)

echo "$JSON" | python3 -m json.tool >/dev/null 2>&1 || {
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

# Should have resolutions
check_json "len(d[0]['resolutions']) >= 1" "True"

# Resolution command should be "npm install bcrypt" (first successful same-binary cmd)
check_json "d[0]['resolutions'][0]['command']" "npm install bcrypt"

# ─── Test 5: --stdin reads from pipe ───
STDIN_OUTPUT=$(echo "Cannot find module bcrypt" | "$RT" resolve --stdin 2>&1)
echo "$STDIN_OUTPUT" | grep -q "matching error pattern" || {
  echo "FAIL: --stdin should find matching patterns"
  echo "$STDIN_OUTPUT"
  exit 1
}

# ─── Test 6: short input rejected ───
SHORT=$("$RT" resolve "err" 2>&1 || true)
echo "$SHORT" | grep -q "Error input too short" || {
  echo "FAIL: should show exact error: 'Error input too short'"
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
# Use --json to verify filtering actually works, not just exit code
# Capture stdout only (stderr has no-match messages that corrupt JSON)
CMD_NPM=$("$RT" resolve "Cannot find module" --cmd npm --json 2>/dev/null)
echo "$CMD_NPM" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert len(d) >= 1, 'Expected at least 1 result for --cmd npm'
" || {
  echo "FAIL: --cmd npm should find bcrypt error"
  echo "$CMD_NPM"
  exit 1
}

# --cmd curl with a bcrypt query should return no results (empty stdout)
CMD_CURL=$("$RT" resolve "Cannot find module" --cmd curl --json 2>/dev/null || true)
if [ -n "$CMD_CURL" ]; then
  echo "$CMD_CURL" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert len(d) == 0, f'Expected 0 results for --cmd curl on bcrypt query, got {len(d)}'
" || {
    echo "FAIL: --cmd curl on bcrypt query should return empty results"
    echo "$CMD_CURL"
    exit 1
  }
fi

# --cmd curl with connection refused query should find the curl error
CMD_CURL_CONN=$("$RT" resolve "connection refused" --cmd curl --json 2>/dev/null)
echo "$CMD_CURL_CONN" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert len(d) >= 1, 'Expected at least 1 result for --cmd curl on connection refused'
" || {
  echo "FAIL: --cmd curl should find connection refused error"
  echo "$CMD_CURL_CONN"
  exit 1
}

# ─── Test 9: resolve in a different project auto-widens to global ───
cd "$TMPDIR"
mkdir other_project && cd other_project
git init -q

OTHER_OUTPUT=$("$RT" resolve "Cannot find module" 2>&1)

# Should show the auto-widen message
echo "$OTHER_OUTPUT" | grep -q "showing global results" || {
  echo "FAIL: should show 'showing global results' when auto-widening"
  echo "$OTHER_OUTPUT"
  exit 1
}

# Should still find the bcrypt error pattern with correct occurrence count
echo "$OTHER_OUTPUT" | grep -q "3 time" || {
  echo "FAIL: auto-widened results should show '3 time(s)' for bcrypt error"
  echo "$OTHER_OUTPUT"
  exit 1
}

# Go back to original workspace for remaining tests
cd "$WORKSPACE"

# ─── Test 10: Resolution content is correct (core value verification) ───
RES_JSON=$("$RT" resolve "Cannot find module bcrypt" --json 2>/dev/null)
echo "$RES_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert len(d) >= 1, 'Expected at least 1 pattern'
# find_resolution picks first successful same-binary (npm) command after failure
# which is 'npm install bcrypt' (comes before 'npm test' re-run)
resolutions = d[0]['resolutions']
assert len(resolutions) >= 1, f'Expected resolutions, got {resolutions}'
assert resolutions[0]['command'] == 'npm install bcrypt', \
    f\"Expected resolution 'npm install bcrypt', got '{resolutions[0]['command']}'\"
assert resolutions[0]['count'] == 3, \
    f\"Expected count 3, got {resolutions[0]['count']}\"
" || {
  echo "FAIL: resolution content verification failed"
  echo "$RES_JSON" | python3 -m json.tool 2>/dev/null || echo "$RES_JSON"
  exit 1
}

# ─── Test 11: Unresolved error has empty resolutions in JSON ───
UNRES_JSON=$("$RT" resolve "connection refused" --json 2>/dev/null)
echo "$UNRES_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert len(d) >= 1, 'Expected at least 1 pattern for connection refused'
assert d[0]['resolutions'] == [], \
    f\"Expected empty resolutions, got {d[0]['resolutions']}\"
assert d[0]['success_rate'] == 0.0, \
    f\"Expected success_rate 0.0, got {d[0]['success_rate']}\"
" || {
  echo "FAIL: unresolved error should have empty resolutions and 0.0 success_rate"
  echo "$UNRES_JSON" | python3 -m json.tool 2>/dev/null || echo "$UNRES_JSON"
  exit 1
}

# ─── Test 12: Ranking order (resolved before unresolved) ───
# "Cannot find module" matches both bcrypt (resolved, 3 occ) and express (unresolved, 1 occ)
RANK_JSON=$("$RT" resolve "Cannot find module" --json 2>/dev/null)
echo "$RANK_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert len(d) >= 2, f'Expected at least 2 patterns, got {len(d)}'
# First pattern should have higher success_rate (resolved)
assert d[0]['success_rate'] >= d[1]['success_rate'], \
    f\"Ranking broken: first={d[0]['success_rate']}, second={d[1]['success_rate']}\"
# First should be bcrypt (resolved, success_rate=1.0)
assert d[0]['success_rate'] == 1.0, \
    f\"Expected first pattern success_rate 1.0, got {d[0]['success_rate']}\"
assert d[0]['occurrences'] == 3, \
    f\"Expected first pattern occurrences 3, got {d[0]['occurrences']}\"
# Second should be express (unresolved, success_rate=0.0)
assert d[1]['success_rate'] == 0.0, \
    f\"Expected second pattern success_rate 0.0, got {d[1]['success_rate']}\"
" || {
  echo "FAIL: ranking order should be resolved (high success_rate) before unresolved"
  echo "$RANK_JSON" | python3 -m json.tool 2>/dev/null || echo "$RANK_JSON"
  exit 1
}

# ─── Test 13: Error normalization (paths and line numbers) ───
# Seeded error: "Error: Cannot read file /home/user/project/src/index.js:42:10"
# Search for the error text (avoid raw paths in query — colons break FTS5 syntax)
# Then verify the normalized error_pattern replaces paths with <path> and line:col with :<line>:
NORM_JSON=$("$RT" resolve "Cannot read file" --json 2>/dev/null)
echo "$NORM_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert len(d) >= 1, \
    'Normalization test: should find the seeded \"Cannot read file\" error'
assert d[0]['occurrences'] >= 1, \
    f\"Expected at least 1 occurrence, got {d[0]['occurrences']}\"
# The normalized error_pattern should have paths replaced with <path>
# and line:col replaced with :<line>:
pattern = d[0]['error_pattern']
assert '<path>' in pattern, \
    f\"Normalization should replace file paths with <path>, got: '{pattern}'\"
assert ':<line>:' in pattern, \
    f\"Normalization should replace line:col with :<line>:, got: '{pattern}'\"
# Original path fragments should NOT appear in the normalized pattern
assert '/home/user/project' not in pattern, \
    f\"Original path should be normalized away, got: '{pattern}'\"
" || {
  echo "FAIL: error normalization should replace paths and line numbers"
  echo "$NORM_JSON" | python3 -m json.tool 2>/dev/null || echo "$NORM_JSON"
  exit 1
}

# ─── Test 14: FTS fallback to word-overlap matching ───
# TODO: The fallback to 60% word-overlap matching (normalized_error_matches) is
# hard to trigger deterministically. FTS typically catches all current test
# queries. To test this path properly, we'd need to seed an error with words
# that FTS doesn't index well and search with a partial overlap. Leaving as a
# gap until we can reliably bypass FTS without implementation coupling.

# ─── Test 15: Full Claude Code capture + resolve integration ───
# TODO: This requires shell integration + an actual Claude Code session to
# capture tool events end-to-end. See eval/tests/resolve_integration.sh
# (to be created) for the full integration test that validates:
# - Real command capture via shell hooks
# - Claude Code tool_response events (file edits as fix actions)
# - resolve correctly identifying the fix from captured interaction

echo "PASS"
