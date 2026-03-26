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

"$RT" proxy echo "seed data" > /dev/null 2>&1 || true

# ASCII table output
TABLE=$("$RT" sql "SELECT id, command FROM events" 2>/dev/null)
echo "$TABLE" | grep -q "id" || { echo "FAIL: missing column header"; exit 1; }
echo "$TABLE" | grep -q -- "---" || { echo "FAIL: missing separator"; exit 1; }
echo "$TABLE" | grep -q "echo" || { echo "FAIL: missing data"; exit 1; }

# JSON output
JSON=$("$RT" sql "SELECT id, command FROM events" --json 2>/dev/null)
echo "$JSON" | grep -q '"command"' || { echo "FAIL: JSON output malformed"; exit 1; }

echo "PASS"
