#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
if [[ -n "${RT_BIN:-}" ]]; then RT="$RT_BIN"; else
    cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null
    RT="$REPO_ROOT/target/release/rt"
fi

ORIG_HOME="$HOME"
TMPDIR=$(mktemp -d)
trap 'export HOME="$ORIG_HOME"; rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"

mkdir -p "$TMPDIR/my-project"
cd "$TMPDIR/my-project"

"$RT" proxy echo test > /dev/null 2>&1 || true

RESULT=$("$RT" sql "SELECT id, name FROM sessions LIMIT 1" --json 2>/dev/null)
echo "$RESULT" | grep -q "my-project" || { echo "FAIL: session not auto-created with dir name"; exit 1; }

echo "PASS"
