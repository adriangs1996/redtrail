#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null
RT="$REPO_ROOT/target/release/rt"
NMAP_FIXTURE="$REPO_ROOT/eval/tests/fixtures/nmap-scan.txt"

# Isolate: override HOME so ~/.redtrail/ goes to a temp dir
ORIG_HOME="$HOME"
TMPDIR=$(mktemp -d)
trap 'export HOME="$ORIG_HOME"; rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
cd "$TMPDIR"

# Setup workspace with target
"$RT" init --target 10.10.10.42 2>/dev/null 1>/dev/null

# Configure to use claude-code provider with auto_extract enabled
"$RT" config set general.llm_provider claude-code 2>/dev/null
"$RT" config set general.auto_extract true 2>/dev/null

# Ingest nmap scan — this triggers extraction via claude-code provider
"$RT" eat "$NMAP_FIXTURE" 2>/dev/null

# Wait for async extraction to complete (spawned subprocess)
# The extraction runs as `rt pipeline extract <cmd_id>` in the background
for i in $(seq 1 60); do
    STATUS=$("$RT" sql --json "SELECT extraction_status FROM command_history WHERE id = 1" 2>/dev/null || echo "")
    if echo "$STATUS" | grep -q '"done"'; then
        break
    fi
    sleep 2
done

# Assert: extraction completed
STATUS=$("$RT" sql --json "SELECT extraction_status FROM command_history WHERE id = 1" 2>/dev/null)
echo "$STATUS" | grep -q '"done"' || { echo "FAIL: extraction not completed (status: $STATUS)"; exit 1; }

# Assert: host was extracted (10.10.10.42)
HOSTS=$("$RT" kb hosts --json 2>/dev/null)
echo "$HOSTS" | grep -q "10.10.10.42" || { echo "FAIL: host 10.10.10.42 not found in KB"; exit 1; }

# Assert: ports were extracted (at least ssh on 22 and http on 80)
PORTS=$("$RT" kb ports --json 2>/dev/null)
echo "$PORTS" | grep -q "22" || { echo "FAIL: port 22 not found"; exit 1; }
echo "$PORTS" | grep -q "80" || { echo "FAIL: port 80 not found"; exit 1; }

# Assert: services were identified
echo "$PORTS" | grep -q "ssh" || { echo "FAIL: ssh service not identified"; exit 1; }
echo "$PORTS" | grep -q -i "http\|apache" || { echo "FAIL: http service not identified"; exit 1; }

echo "PASS"
