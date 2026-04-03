#!/usr/bin/env bash
# Live test: capture git commands, extract entities, verify context command output
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

# Initialize a git repo
mkdir -p "$TMPDIR/project"
cd "$TMPDIR/project"
git init -q
git config user.email "test@test.com"
git config user.name "Test"
echo "fn main() {}" > main.rs
git add main.rs
git commit -q -m "initial commit"
echo "// second" >> main.rs
git add main.rs
git commit -q -m "second commit"
echo "// modified" >> main.rs

# .zshrc
cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

# Run git commands
cat >"$TMPDIR/commands.txt" <<EOF
cd "$TMPDIR/project"
git status
git log --oneline
git branch
git remote -v
exit
EOF

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Run extraction
"$RT" extract 2>/dev/null

# Test context markdown output
CONTEXT=$("$RT" context --repo "$TMPDIR/project" 2>/dev/null)
echo "$CONTEXT" | grep -q "Branch" || {
  echo "FAIL: context missing Branch section"
  echo "Got: $CONTEXT"
  exit 1
}

# Test context JSON output
JSON=$("$RT" context --repo "$TMPDIR/project" --format json 2>/dev/null)
echo "$JSON" | python3 -c "import sys, json; json.load(sys.stdin)" 2>/dev/null || {
  echo "FAIL: context --format json is not valid JSON"
  echo "Got: $JSON"
  exit 1
}

echo "PASS"
