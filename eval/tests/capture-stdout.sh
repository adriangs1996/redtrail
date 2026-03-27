#!/usr/bin/env bash
# Live test: capture a command with stdout via --stdout-file,
# verify stdout is stored and searchable in the DB
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

SESSION_ID=$("$RT" session-id 2>/dev/null)

# Simulate what redtrail tee would produce: a temp file with header + content
STDOUT_FILE="$TMPDIR/rt-out-test"
cat > "$STDOUT_FILE" <<'EOF'
ts_start:1000
ts_end:1002
truncated:false

total 4
drwxr-xr-x 2 user user 4096 Mar 27 10:00 src
-rw-r--r-- 1 user user  156 Mar 27 10:00 Cargo.toml
EOF

"$RT" capture \
    --session-id "$SESSION_ID" \
    --command "ls -la" \
    --exit-code 0 \
    --shell zsh \
    --hostname testbox \
    --stdout-file "$STDOUT_FILE" \
    2>/dev/null

# Verify temp file was cleaned up
[ ! -f "$STDOUT_FILE" ] || { echo "FAIL: temp file should be deleted by capture"; exit 1; }

# Verify stdout is stored
STDOUT_CHECK=$("$RT" query "SELECT stdout FROM commands WHERE command_binary = 'ls' AND stdout IS NOT NULL" --json 2>/dev/null)
echo "$STDOUT_CHECK" | grep -q "Cargo.toml" || { echo "FAIL: stdout not captured"; exit 1; }

# Verify timestamps came from the header
TS_CHECK=$("$RT" query "SELECT timestamp_start, timestamp_end FROM commands LIMIT 1" --json 2>/dev/null)
echo "$TS_CHECK" | grep -q "1000" || { echo "FAIL: timestamp_start should be from header"; exit 1; }
echo "$TS_CHECK" | grep -q "1002" || { echo "FAIL: timestamp_end should be from header"; exit 1; }

# Verify command is searchable via FTS (command string search)
SEARCH=$("$RT" history --search "ls" --json 2>/dev/null)
echo "$SEARCH" | grep -q "ls" || { echo "FAIL: command should be searchable via FTS"; exit 1; }

echo "PASS"
