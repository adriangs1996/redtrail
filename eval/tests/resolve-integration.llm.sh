#!/usr/bin/env bash
# Live test: Full Claude Code integration — drives a real claude -p session
# that encounters an error, investigates, fixes, and re-runs.
# Verifies that `resolve` correctly identifies the resolution and that
# `agent-report` correctly traces the error-fix sequence.
#
# This differs from resolve.sh (which tests resolve in isolation with manual
# ingest calls) by testing the full pipeline through Claude Code hooks.
#
# Requires: CLAUDE_CODE_OAUTH_TOKEN set in environment.
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"
export NO_COLOR=1

# Skip if no OAuth token available
if [ -z "${CLAUDE_CODE_OAUTH_TOKEN:-}" ]; then
  echo "SKIP: CLAUDE_CODE_OAUTH_TOKEN not set"
  exit 0
fi

# Initialize a workspace with git
WORKSPACE="$TMPDIR/workspace"
mkdir -p "$WORKSPACE/src"
cd "$WORKSPACE"
git init -q
git config user.email "test@test.com"
git config user.name "Test"

# Install redtrail hooks for Claude Code
"$RT" setup-hooks

# Verify hooks were installed
[ -f ".claude/hooks/redtrail-capture.sh" ] || {
  echo "FAIL: hook script not created"
  exit 1
}
grep -q "redtrail" .claude/settings.json || {
  echo "FAIL: settings.json missing redtrail hooks"
  exit 1
}

# ─── Create a Python file with an obvious syntax error ───
# The error is unambiguous: missing colon on the def line.
cat > "$WORKSPACE/app.py" <<'PYEOF'
def greet(name)
    return f"Hello, {name}!"

if __name__ == "__main__":
    print(greet("world"))
PYEOF

# ─── Drive Claude Code to find and fix the error ───
# Single claude -p call: run the script, see the error, fix it, re-run.
claude -p \
  --dangerously-skip-permissions \
  "Run python3 app.py in this directory. It will fail with a syntax error. Read the file, fix the bug, then run python3 app.py again to confirm it works. Use the Bash tool to run python3 and the Read/Edit tools to inspect and fix the file." \
  > /dev/null 2>&1 || {
  echo "FAIL: claude -p command failed"
  exit 1
}

# Give async hooks time to flush
sleep 3

# ═══════════════════════════════════════════════════════════════
# Tests
# ═══════════════════════════════════════════════════════════════

# ─── Test 1: resolve finds the syntax error and reports it as resolved ───
RESOLVE_JSON=$("$RT" resolve "SyntaxError" --json 2>/dev/null)
echo "$RESOLVE_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert len(d) >= 1, f'Expected at least 1 pattern for SyntaxError, got {len(d)}'
p = d[0]
assert p['occurrences'] >= 1, f\"Expected >= 1 occurrence, got {p['occurrences']}\"
assert p['success_rate'] > 0, \
    f\"SyntaxError should be resolved (success_rate > 0), got {p['success_rate']}\"
assert len(p['resolutions']) >= 1, \
    f\"Expected at least 1 resolution, got {p['resolutions']}\"
" || {
  echo "FAIL: Test 1 — resolve should find SyntaxError as resolved"
  echo "resolve --json output:"
  echo "$RESOLVE_JSON" | python3 -m json.tool 2>/dev/null || echo "$RESOLVE_JSON"
  echo "DEBUG: all commands in db:"
  "$RT" query "SELECT id, tool_name, command_raw, exit_code, stderr FROM commands ORDER BY id" 2>/dev/null || true
  exit 1
}

# ─── Test 2: agent-report shows at least 1 resolved error sequence ───
REPORT_JSON=$("$RT" agent-report --json 2>&1)
echo "$REPORT_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)

# Should have commands from the Claude Code session
assert d['commands']['total'] >= 3, \
    f\"Expected >= 3 commands (fail + fix + pass), got {d['commands']['total']}\"

# Should have at least 1 error sequence
assert len(d['errors']) >= 1, \
    f\"Expected at least 1 error sequence, got {len(d['errors'])}\"

# At least one error should be resolved
resolved = [e for e in d['errors'] if e['resolved']]
assert len(resolved) >= 1, \
    f\"Expected at least 1 resolved error, got {len(resolved)} out of {len(d['errors'])}\"
" || {
  echo "FAIL: Test 2 — agent-report should show resolved error sequence"
  echo "agent-report --json output:"
  echo "$REPORT_JSON" | python3 -m json.tool 2>/dev/null || echo "$REPORT_JSON"
  exit 1
}

# ─── Test 3: agent-report fix_actions include an Edit or Write ───
echo "$REPORT_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)

resolved = [e for e in d['errors'] if e['resolved']]
assert len(resolved) >= 1, 'No resolved errors to check fix_actions'

err = resolved[0]
fix_actions = err.get('fix_actions', [])
has_edit_or_write = any('Edit' in a or 'Write' in a for a in fix_actions)
assert has_edit_or_write, \
    f\"Fix actions should include an Edit or Write, got {fix_actions}\"
" || {
  echo "FAIL: Test 3 — fix_actions should include Edit or Write"
  echo "$REPORT_JSON" | python3 -m json.tool 2>/dev/null || echo "$REPORT_JSON"
  exit 1
}

# ─── Test 4: Cross-validation — resolve and agent-report agree ───
echo "$RESOLVE_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert d[0]['success_rate'] > 0, 'resolve says not resolved'
" || {
  echo "FAIL: Test 4a — resolve says error is resolved"
  exit 1
}
echo "$REPORT_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
resolved = [e for e in d['errors'] if e['resolved']]
assert len(resolved) >= 1, 'agent-report says no resolved errors'
" || {
  echo "FAIL: Test 4b — agent-report says error is resolved"
  exit 1
}

# ─── Test 5: source is claude_code ───
echo "$REPORT_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert d['source'] == 'claude_code', \
    f\"Expected source 'claude_code', got '{d['source']}'\"
" || {
  echo "FAIL: Test 5 — source should be claude_code"
  exit 1
}

echo "PASS"
