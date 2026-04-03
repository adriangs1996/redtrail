#!/usr/bin/env bash
# Live test: capture git commands, run extraction, verify entities created
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

# Initialize a git repo for context
mkdir -p "$TMPDIR/project"
cd "$TMPDIR/project"
git init -q
git config user.email "test@test.com"
git config user.name "Test"
echo "fn main() {}" > main.rs
git add main.rs
git commit -q -m "initial commit"
echo "// changed" >> main.rs

# .zshrc that sources our hooks
cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

# Commands: run git status to get output that the extractor can parse
cat >"$TMPDIR/commands.txt" <<EOF
cd "$TMPDIR/project"
git status
git branch
exit
EOF

# Run in a real PTY so hooks fire
HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Verify capture worked
HISTORY=$("$RT" history --json 2>/dev/null)
echo "$HISTORY" | grep -q "git status" || {
  echo "FAIL: git status not in history"
  exit 1
}

# Run extraction
"$RT" extract 2>/dev/null || {
  echo "FAIL: extract command failed"
  exit 1
}

# Verify entities were created
ENTITIES=$("$RT" entities --json 2>/dev/null)
echo "$ENTITIES" | grep -q "git_file" || {
  echo "FAIL: no git_file entities found after extraction"
  echo "Entities output: $ENTITIES"
  exit 1
}

echo "PASS"
