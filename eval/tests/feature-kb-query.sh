#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null
RT="$REPO_ROOT/target/release/rt"

# Isolate: override HOME so ~/.redtrail/ goes to a temp dir
ORIG_HOME="$HOME"
TMPDIR=$(mktemp -d)
trap 'export HOME="$ORIG_HOME"; rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
cd "$TMPDIR"

# Setup workspace
"$RT" init --target 10.10.10.1 2>/dev/null 1>/dev/null

# Get the session_id for SQL inserts
SESSION_ID=$("$RT" sql --json "SELECT id FROM sessions WHERE active = 1 LIMIT 1" 2>/dev/null | grep -o '"id": *"[^"]*"' | head -1 | sed 's/.*: *"//;s/"//')
[[ -n "$SESSION_ID" ]] || { echo "FAIL: could not get session_id"; exit 1; }

# Insert test data with session_id
"$RT" sql "INSERT INTO hosts (session_id, ip, hostname, os, status) VALUES ('$SESSION_ID', '10.10.10.1', 'target', 'Linux', 'up')" 2>/dev/null
"$RT" sql "INSERT INTO ports (session_id, host_id, port, protocol, service, version) VALUES ('$SESSION_ID', 1, 22, 'tcp', 'ssh', 'OpenSSH 8.9')" 2>/dev/null

# Test: kb hosts returns data
HOSTS=$("$RT" kb hosts --json 2>/dev/null)
echo "$HOSTS" | grep -q "10.10.10.1" || { echo "FAIL: host not found"; exit 1; }

# Test: kb ports returns data
PORTS=$("$RT" kb ports --json 2>/dev/null)
echo "$PORTS" | grep -q "ssh" || { echo "FAIL: port not found"; exit 1; }

# Test: status shows correct target
STATUS=$("$RT" status --json 2>/dev/null)
echo "$STATUS" | grep -q "10.10.10.1" || { echo "FAIL: status target wrong"; exit 1; }

# Test: search returns results (search by host IP, which is in the searchable fields)
SEARCH=$("$RT" kb search "10.10.10.1" --json 2>/dev/null)
echo "$SEARCH" | grep -q "10.10.10.1" || { echo "FAIL: search returned nothing"; exit 1; }

echo "PASS"
