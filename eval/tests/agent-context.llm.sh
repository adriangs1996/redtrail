#!/usr/bin/env bash
# Live test: agent-context smart mode (--smart flag, requires Ollama).
# Verifies that LLM-powered summarization produces narrative output
# with the correct section structure.
set -euo pipefail

RT="${RT_BIN:-/usr/local/bin/redtrail}"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"
export NO_COLOR=1

# Configure LLM (Ollama must be running)
mkdir -p "$TMPDIR/.config/redtrail"
cat > "$TMPDIR/.config/redtrail/config.yaml" <<EOF
llm:
  enabled: true
  provider: ollama
  ollama:
    url: http://localhost:11434
    model: gemma4
  timeout_seconds: 30
  max_input_chars: 8192
EOF

# Initialize a workspace with git
WORKSPACE="$TMPDIR/workspace"
mkdir -p "$WORKSPACE/src"
cd "$WORKSPACE"
git init -q
git config user.email "test@test.com"
git config user.name "Test"
echo "fn main() {}" > src/main.rs
git add -A && git commit -q -m "feat: initial project setup"

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
# DATA SETUP — a realistic coding session
# ═══════════════════════════════════════════════════════════════════════

# Session A: implementing user authentication
ingest_event "Read" '{"file_path": "src/main.rs"}' "session-A"
sleep 1
ingest_event "Write" '{"file_path": "src/auth.rs", "content": "pub fn login() {}"}' "session-A"
sleep 1
ingest_event "Edit" '{"file_path": "src/main.rs", "old_string": "fn main", "new_string": "mod auth;\nfn main"}' "session-A"
sleep 1
ingest_event "Bash" '{"command": "cargo build"}' "session-A" '{"stdout": "Compiling project v0.1.0", "exitCode": 0}'
sleep 1
ingest_event "Bash" '{"command": "cargo test"}' "session-A" 'null' '"error[E0433]: failed to resolve: could not find `auth` in the root"'
sleep 1
ingest_event "Edit" '{"file_path": "src/auth.rs", "old_string": "pub fn login", "new_string": "pub fn login(user: &str)"}' "session-A"
sleep 1
ingest_event "Bash" '{"command": "cargo test"}' "session-A" '{"stdout": "test result: ok. 3 passed", "exitCode": 0}'

# ═══════════════════════════════════════════════════════════════════════
# SMART MODE ASSERTIONS
# ═══════════════════════════════════════════════════════════════════════
MD=$("$RT" agent-context --smart 2>"$TMPDIR/stderr.log")

# Check for smart mode section headers
echo "$MD" | grep -q "# Project Context (RedTrail)" || fail "missing main heading"
echo "$MD" | grep -q "## Where You Left Off" || fail "missing 'Where You Left Off' section"
echo "$MD" | grep -q "## Open Work" || fail "missing 'Open Work' section"

# The "Where You Left Off" section should contain LLM-generated narrative
# (not raw command output). It should mention files or concepts, not just tool names.
WHERE_LEFT_OFF=$(echo "$MD" | sed -n '/## Where You Left Off/,/^## /p' | grep -v "^##")
WORD_COUNT=$(echo "$WHERE_LEFT_OFF" | wc -w | tr -d ' ')
if [ "$WORD_COUNT" -lt 10 ]; then
  fail "Where You Left Off section too short ($WORD_COUNT words) — LLM may have failed"
fi

# Open Work should mention git state or clean state
echo "$MD" | sed -n '/## Open Work/,/^## /p' | grep -qiE "uncommitted|clean|failing|error" || \
  fail "Open Work should mention git/test state"

# Overall output should be reasonable length (not empty, not huge)
CHAR_COUNT=${#MD}
if [ "$CHAR_COUNT" -lt 100 ]; then
  fail "Smart mode output suspiciously short ($CHAR_COUNT chars)"
fi
if [ "$CHAR_COUNT" -gt 12000 ]; then
  fail "Smart mode output too long ($CHAR_COUNT chars), should be <3000 tokens"
fi

# ═══════════════════════════════════════════════════════════════════════
# GRACEFUL DEGRADATION: --smart with bad LLM config should still produce output
# ═══════════════════════════════════════════════════════════════════════
cat > "$TMPDIR/.config/redtrail/config.yaml" <<EOF
llm:
  enabled: true
  provider: ollama
  ollama:
    url: http://localhost:99999
    model: nonexistent
  timeout_seconds: 3
EOF

DEGRADED=$("$RT" agent-context --smart 2>/dev/null)
echo "$DEGRADED" | grep -q "# Project Context" || \
  fail "Smart mode with bad LLM config should still produce output"
echo "$DEGRADED" | grep -q "## Where You Left Off" || \
  fail "Smart mode degraded should still have Where You Left Off section"

# ═══════════════════════════════════════════════════════════════════════
# RESULT
# ═══════════════════════════════════════════════════════════════════════
if [ "$FAIL_COUNT" -gt 0 ]; then
  echo ""
  echo "FAILED: $FAIL_COUNT assertion(s) failed"
  exit 1
fi

echo "PASS"
