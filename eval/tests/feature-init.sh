#!/usr/bin/env bash
set -euo pipefail

# Build the binary
REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null
RT="$REPO_ROOT/target/release/rt"

# Isolate: override HOME so ~/.redtrail/ goes to a temp dir
ORIG_HOME="$HOME"
TMPDIR=$(mktemp -d)
trap 'export HOME="$ORIG_HOME"; rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"

# Test: init creates workspace (database-backed at ~/.redtrail/)
cd "$TMPDIR"
"$RT" init --target 10.10.10.1 --goal capture-flags --scope 10.10.10.0/24 \
    2>/dev/null 1>/dev/null

# Assert: global database exists at ~/.redtrail/
RT_DIR="$TMPDIR/.redtrail"
[[ -d "$RT_DIR" ]] || { echo "FAIL: .redtrail dir missing"; exit 1; }
[[ -f "$RT_DIR/redtrail.db" ]] || { echo "FAIL: db missing"; exit 1; }

# Assert: session was created in database
SESSION=$("$RT" session active --json 2>/dev/null || true)
echo "$SESSION" | grep -q "10.10.10.1" || { echo "FAIL: session not created or target missing"; exit 1; }

echo "PASS"
