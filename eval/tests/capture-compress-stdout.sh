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

# Create a helper script whose output contains a unique marker NOT present in
# the command line itself. This lets us verify FTS indexes compressed stdout,
# not just command_raw.
cat >"$TMPDIR/gen.sh" <<'SCRIPT'
printf 'COMPRESSMARKER_7f3a %0500d\n' 0
SCRIPT

cat >"$TMPDIR/commands.txt" <<CMDS
bash $TMPDIR/gen.sh
exit 0
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# ── 1. Stdout was compressed (blob column populated, text column NULL) ──
COMPRESSED=$("$RT" query "SELECT stdout_compressed IS NOT NULL as has_blob, stdout IS NULL as text_null, length(stdout_compressed) as blob_len FROM commands WHERE command_binary = 'bash'" --json 2>/dev/null)
echo "$COMPRESSED" | grep -q '"has_blob": 1' || echo "$COMPRESSED" | grep -q '"has_blob":1' || {
  echo "FAIL: stdout_compressed should be populated. Got: $COMPRESSED"
  exit 1
}
echo "$COMPRESSED" | grep -q '"text_null": 1' || echo "$COMPRESSED" | grep -q '"text_null":1' || {
  echo "FAIL: stdout text column should be NULL when compressed. Got: $COMPRESSED"
  exit 1
}

# ── 1b. The blob is actually compressed (smaller than original output) ──
# The original output is ~520 bytes ("COMPRESSMARKER_7f3a " + 500 zeros + newline).
# Zlib should compress repetitive zeros well — the blob must be significantly smaller.
BLOB_LEN=$(echo "$COMPRESSED" | grep -o '"blob_len": *[0-9]*' | grep -o '[0-9]*$')
test -n "$BLOB_LEN" || { echo "FAIL: could not read blob_len. Got: $COMPRESSED"; exit 1; }
test "$BLOB_LEN" -lt 200 || {
  echo "FAIL: compressed blob ($BLOB_LEN bytes) should be much smaller than original (~520 bytes). Compression may not be working."
  exit 1
}

# ── 1c. The blob starts with a zlib header (first byte 0x78) ──
# Zlib default compression starts with 0x78 0x9C. Query the hex of the first byte.
FIRST_BYTE=$("$RT" query "SELECT hex(substr(stdout_compressed, 1, 1)) as hdr FROM commands WHERE command_binary = 'bash'" --json 2>/dev/null)
echo "$FIRST_BYTE" | grep -q '"hdr": "78"' || echo "$FIRST_BYTE" | grep -q '"hdr":"78"' || {
  echo "FAIL: compressed blob should start with zlib header 0x78. Got: $FIRST_BYTE"
  exit 1
}

# ── 2. Full content is recoverable via get_commands (transparent decompression) ──
# history --json doesn't include stdout, so use redtrail query on a view that
# triggers get_commands logic. Since query hits raw SQL (no decompression), we
# use the history table output instead. Verify via --verbose (non-json, table mode).
VERBOSE=$("$RT" history --verbose 2>/dev/null)
echo "$VERBOSE" | grep -q 'COMPRESSMARKER_7f3a' || {
  echo "FAIL: decompressed stdout should appear in verbose history output. Got:"
  echo "$VERBOSE"
  exit 1
}

# ── 3. FTS search finds content that ONLY exists in compressed stdout ──
# "COMPRESSMARKER_7f3a" does NOT appear in command_raw (which is "bash /tmp/.../gen.sh"),
# so this proves FTS indexed the stdout content, not just the command.
SEARCH=$("$RT" history --search COMPRESSMARKER_7f3a 2>/dev/null)
echo "$SEARCH" | grep -q 'bash' || {
  echo "FAIL: FTS search should find the command by stdout-only content. Got: $SEARCH"
  exit 1
}

# ── 4. Command status should be 'finished' ──
STATUS_CHECK=$("$RT" query "SELECT status FROM commands WHERE command_binary = 'bash' LIMIT 1" --json 2>/dev/null)
echo "$STATUS_CHECK" | grep -q "finished" || {
  echo "FAIL: command status not 'finished'. Got: $STATUS_CHECK"
  exit 1
}

echo "PASS"
