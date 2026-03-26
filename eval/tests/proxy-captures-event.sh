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
cd "$TMPDIR"

"$RT" proxy echo "hello redtrail" > /dev/null 2>&1 || true

RESULT=$("$RT" sql "SELECT command, extraction_status FROM events LIMIT 1" --json 2>/dev/null)
echo "$RESULT" | grep -q "echo" || { echo "FAIL: event not stored"; exit 1; }
echo "$RESULT" | grep -q "stored" || { echo "FAIL: extraction_status should be stored"; exit 1; }

echo "PASS"
