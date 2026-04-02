#!/usr/bin/env bash
# Live test: on_detect=block deletes command row when secret appears in stdout stream.
# The secret is in a file (not command_raw) so capture start allows the command.
# Tee detects the secret during its 1s periodic flush and deletes the row.
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

mkdir -p "$TMPDIR/.config/redtrail"
cat >"$TMPDIR/.config/redtrail/config.yaml" <<'CONF'
secrets:
  on_detect: block
CONF

# Secret lives in a file — command_raw will be "cat <path>" (clean)
echo "key=AKIAIOSFODNN7EXAMPLE" > "$TMPDIR/secret-file.txt"

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

# cat the secret file, then sleep to give tee time for a 1s flush cycle
cat >"$TMPDIR/commands.txt" <<CMDS
cat $TMPDIR/secret-file.txt; sleep 3
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# The command row should be deleted (on_detect=block saw secret in stdout)
COUNT=$("$RT" query "SELECT COUNT(*) as cnt FROM commands WHERE command_raw LIKE '%secret-file%'" --json 2>/dev/null)
echo "$COUNT" | grep -qE '"cnt":\s*0' || {
  echo "FAIL: command with secret in output should have been deleted. Got: $COUNT"
  exit 1
}

echo "PASS"
