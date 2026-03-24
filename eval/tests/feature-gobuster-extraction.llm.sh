#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
if [[ -n "${RT_BIN:-}" ]]; then
    RT="$RT_BIN"
else
    cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null
    RT="$REPO_ROOT/target/release/rt"
fi
GOBUSTER_FIXTURE="$REPO_ROOT/eval/tests/fixtures/gobuster-scan.txt"

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

# Ingest gobuster scan — this triggers extraction via claude-code provider
"$RT" eat "$GOBUSTER_FIXTURE" 2>/dev/null

# Wait for async extraction to complete
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

# Assert: tool detected as gobuster
TOOL=$("$RT" sql --json "SELECT tool FROM command_history WHERE id = 1" 2>/dev/null)
echo "$TOOL" | grep -qi 'gobuster' || { echo "FAIL: tool not detected as gobuster (got: $TOOL)"; exit 1; }

# Assert: web_paths populated — check all 10 paths with expected status codes
check_path() {
    local path="$1"
    local expected_status="$2"
    ROW=$("$RT" sql --json "SELECT status_code, source FROM web_paths WHERE path = '$path'" 2>/dev/null)
    echo "$ROW" | grep -q "status_code" || { echo "FAIL: path $path not found in web_paths"; exit 1; }
    echo "$ROW" | grep -q "\"$expected_status\"" || { echo "FAIL: path $path expected status $expected_status (got: $ROW)"; exit 1; }
}

check_path "/admin"          "301"
check_path "/api"            "200"
check_path "/backup"         "403"
check_path "/cgi-bin"        "403"
check_path "/config"         "401"
check_path "/dashboard"      "302"
check_path "/images"         "301"
check_path "/index.html"     "200"
check_path "/robots.txt"     "200"
check_path "/server-status"  "500"

# Assert: source references gobuster
SOURCE=$("$RT" sql --json "SELECT DISTINCT source FROM web_paths" 2>/dev/null)
echo "$SOURCE" | grep -qi 'gobuster' || { echo "FAIL: web_paths source does not reference gobuster"; exit 1; }

echo "PASS"
