#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

COMPOSE_PROJECT="simple-lab-smoke-$$"
PASS=0
FAIL=0
ERRORS=""

cleanup() {
  echo "--- Tearing down ---"
  docker compose -p "$COMPOSE_PROJECT" $COMPOSE_FILES down --volumes --remove-orphans 2>/dev/null || true
}

COMPOSE_FILES="-f docker-compose.yml"
if [ "${CI:-}" = "true" ]; then
  COMPOSE_FILES="$COMPOSE_FILES -f docker-compose.ci.yml"
fi

trap cleanup EXIT

log_pass() { PASS=$((PASS + 1)); echo "  PASS: $1"; }
log_fail() { FAIL=$((FAIL + 1)); ERRORS="${ERRORS}\n  FAIL: $1"; echo "  FAIL: $1"; }

echo "--- Building and starting lab ---"
docker compose -p "$COMPOSE_PROJECT" $COMPOSE_FILES up -d --build --wait --wait-timeout 60

CONTAINER=$(docker compose -p "$COMPOSE_PROJECT" $COMPOSE_FILES ps -q target)
if [ -z "$CONTAINER" ]; then
  echo "ERROR: container not found"
  exit 1
fi

HTTP_PORT=$(docker port "$CONTAINER" 80/tcp | head -1 | cut -d: -f2)
FTP_PORT=$(docker port "$CONTAINER" 21/tcp | head -1 | cut -d: -f2)
SSH_PORT=$(docker port "$CONTAINER" 22/tcp | head -1 | cut -d: -f2)

echo "Ports — HTTP:$HTTP_PORT FTP:$FTP_PORT SSH:$SSH_PORT"

wait_for_port() {
  local port=$1 name=$2 retries=20
  for i in $(seq 1 $retries); do
    if nc -z 127.0.0.1 "$port" 2>/dev/null; then return 0; fi
    sleep 1
  done
  echo "  WARN: $name on port $port not ready after ${retries}s"
  return 1
}

echo ""
echo "=== Test: Container boots ==="
STATE=$(docker inspect -f '{{.State.Status}}' "$CONTAINER")
if [ "$STATE" = "running" ]; then
  log_pass "container is running"
else
  log_fail "container state: $STATE (expected running)"
fi

echo ""
echo "=== Test: HTTP returns 200 ==="
wait_for_port "$HTTP_PORT" "HTTP"
HTTP_CODE=$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:${HTTP_PORT}/" || echo "000")
if [ "$HTTP_CODE" = "200" ]; then
  log_pass "HTTP GET / returned $HTTP_CODE"
else
  log_fail "HTTP GET / returned $HTTP_CODE (expected 200)"
fi

echo ""
echo "=== Test: FTP anonymous login + file listing ==="
wait_for_port "$FTP_PORT" "FTP"
FTP_OUT=$(curl -s --max-time 10 "ftp://127.0.0.1:${FTP_PORT}/pub/" --user "anonymous:" 2>&1 || true)
if echo "$FTP_OUT" | grep -q "backup.sql.gz"; then
  log_pass "FTP anonymous lists backup.sql.gz"
else
  log_fail "FTP anonymous listing missing backup.sql.gz: $FTP_OUT"
fi
if echo "$FTP_OUT" | grep -q "credentials.txt"; then
  log_pass "FTP anonymous lists credentials.txt"
else
  log_fail "FTP anonymous listing missing credentials.txt: $FTP_OUT"
fi

echo ""
echo "=== Test: SSH accepts connections ==="
wait_for_port "$SSH_PORT" "SSH"
SSH_OUT=$(ssh -o StrictHostKeyChecking=no -o BatchMode=yes -o ConnectTimeout=5 \
  -p "$SSH_PORT" labuser@127.0.0.1 echo ok 2>&1 || true)
if echo "$SSH_OUT" | grep -qi "permission denied\|password"; then
  log_pass "SSH accepts connection (auth challenge received)"
elif echo "$SSH_OUT" | grep -q "ok"; then
  log_pass "SSH accepts connection (logged in)"
else
  log_fail "SSH unexpected response: $SSH_OUT"
fi

echo ""
echo "========================"
echo "Results: $PASS passed, $FAIL failed"
if [ "$FAIL" -gt 0 ]; then
  echo -e "\nFailures:$ERRORS"
  exit 1
fi
echo "All smoke tests passed."
