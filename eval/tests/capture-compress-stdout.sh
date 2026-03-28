#!/usr/bin/env bash
# Live test: stdout over max_stdout_bytes is compressed with zlib, fully
# recoverable via query, and still searchable via FTS.
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"
export REDTRAIL_CONFIG="$TMPDIR/config.yaml"

# Set a small limit so the test doesn't need to generate 50KB of output
cat >"$TMPDIR/config.yaml" <<'EOF'
capture:
  max_stdout_bytes: 200
EOF

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

# Generate output well over 200 bytes with a unique marker for FTS search.
# printf is a shell builtin — guaranteed available.
cat >"$TMPDIR/commands.txt" <<'CMDS'
printf 'UNIQUEMARKER_ZLIB %0500d\n' 0
exit 0
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# ── 1. Stdout was compressed (blob column populated, text column NULL) ──
COMPRESSED=$("$RT" query "SELECT stdout_compressed IS NOT NULL as has_blob, stdout IS NULL as text_null FROM commands WHERE command_binary = 'printf'" --json 2>/dev/null)
echo "$COMPRESSED" | grep -q '"has_blob": 1' || echo "$COMPRESSED" | grep -q '"has_blob":1' || {
  echo "FAIL: stdout_compressed should be populated. Got: $COMPRESSED"
  exit 1
}
echo "$COMPRESSED" | grep -q '"text_null": 1' || echo "$COMPRESSED" | grep -q '"text_null":1' || {
  echo "FAIL: stdout text column should be NULL when compressed. Got: $COMPRESSED"
  exit 1
}

# ── 2. Full content is recoverable via history --verbose --json ──
HISTORY=$("$RT" history --verbose --json 2>/dev/null)
echo "$HISTORY" | grep -q 'UNIQUEMARKER_ZLIB' || {
  echo "FAIL: decompressed stdout should contain the unique marker. Got: $HISTORY"
  exit 1
}

# ── 3. FTS search still finds content inside compressed output ──
SEARCH=$("$RT" history --search UNIQUEMARKER_ZLIB --json 2>/dev/null)
echo "$SEARCH" | grep -q 'printf' || {
  echo "FAIL: FTS search should find the command by stdout content. Got: $SEARCH"
  exit 1
}

echo "PASS"
