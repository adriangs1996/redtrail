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

"$RT" proxy echo "unknown tool output" > /dev/null 2>&1 || true

EVENT=$("$RT" sql "SELECT COUNT(*) as count FROM events" --json 2>/dev/null)
echo "$EVENT" | grep -q '"count": 1' || { echo "FAIL: event not stored"; exit 1; }

FACTS=$("$RT" sql "SELECT COUNT(*) as count FROM facts" --json 2>/dev/null)
echo "$FACTS" | grep -q '"count": 0' || { echo "FAIL: unexpected facts for unknown tool"; exit 1; }

echo "PASS"
