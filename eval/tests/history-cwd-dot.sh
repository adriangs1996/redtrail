#!/usr/bin/env bash
# Live test: history --cwd . resolves to current directory
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

WORK_DIR="$TMPDIR/myproject"
mkdir -p "$WORK_DIR"

# .zshrc
cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

# Run commands from different directories
cat >"$TMPDIR/commands.txt" <<EOF
cd $WORK_DIR
echo "in-project-dir"
cd /tmp
echo "in-tmp-dir"
exit
EOF

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# From the project dir, --cwd . should only show commands run there
OUTPUT=$(cd "$WORK_DIR" && "$RT" history --cwd . --json 2>/dev/null)

echo "$OUTPUT" | grep -q "in-project-dir" || {
  echo "FAIL: should show command from project dir"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

echo "$OUTPUT" | grep -q "in-tmp-dir" && {
  echo "FAIL: should NOT show command from /tmp when --cwd . is used from project dir"
  echo "OUTPUT: $OUTPUT"
  exit 1
}

echo "PASS"
