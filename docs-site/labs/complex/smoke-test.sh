#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

COMPOSE_PROJECT="complex-lab-smoke-$$"
PASS=0
FAIL=0
ERRORS=""

cleanup() {
  echo ""
  echo "--- Tearing down ---"
  docker compose -p "$COMPOSE_PROJECT" $COMPOSE_FILES down --volumes --remove-orphans 2>/dev/null || true
}

COMPOSE_FILES="-f docker-compose.yml -f docker-compose.ci.yml"

trap cleanup EXIT

log_pass() { PASS=$((PASS + 1)); echo "  PASS: $1"; }
log_fail() { FAIL=$((FAIL + 1)); ERRORS="${ERRORS}\n  FAIL: $1"; echo "  FAIL: $1"; }

exec_in() {
  local container=$1; shift
  docker exec "$container" "$@"
}

echo "--- Building and starting complex lab ---"
docker compose -p "$COMPOSE_PROJECT" $COMPOSE_FILES up -d --build --wait --wait-timeout 120

# ============================================================
# 1. All 4 containers boot and are healthy
# ============================================================
echo ""
echo "=== Test: All containers boot ==="
for SVC in web01 app01 db01 dc01; do
  CONTAINER=$(docker compose -p "$COMPOSE_PROJECT" $COMPOSE_FILES ps -q "$SVC")
  if [ -z "$CONTAINER" ]; then
    log_fail "$SVC container not found"
    continue
  fi
  STATE=$(docker inspect -f '{{.State.Status}}' "$CONTAINER")
  HEALTH=$(docker inspect -f '{{.State.Health.Status}}' "$CONTAINER" 2>/dev/null || echo "none")
  if [ "$STATE" = "running" ] && [ "$HEALTH" = "healthy" ]; then
    log_pass "$SVC running and healthy"
  else
    log_fail "$SVC state=$STATE health=$HEALTH (expected running/healthy)"
  fi
done

WEB01=$(docker compose -p "$COMPOSE_PROJECT" $COMPOSE_FILES ps -q web01)
APP01=$(docker compose -p "$COMPOSE_PROJECT" $COMPOSE_FILES ps -q app01)
DB01=$(docker compose -p "$COMPOSE_PROJECT" $COMPOSE_FILES ps -q db01)
DC01=$(docker compose -p "$COMPOSE_PROJECT" $COMPOSE_FILES ps -q dc01)

# ============================================================
# 2. Network isolation: web01 has published port, others do not
# ============================================================
echo ""
echo "=== Test: Network isolation ==="

WEB_PUBLISHED=$(docker port "$WEB01" 80/tcp 2>/dev/null || true)
if [ -n "$WEB_PUBLISHED" ]; then
  log_pass "web01 has published HTTP port ($WEB_PUBLISHED)"
else
  log_fail "web01 has no published port for 80/tcp"
fi

for SVC in app01 db01 dc01; do
  CID=$(docker compose -p "$COMPOSE_PROJECT" $COMPOSE_FILES ps -q "$SVC")
  ANY_PORT=$(docker port "$CID" 2>/dev/null || true)
  if [ -z "$ANY_PORT" ]; then
    log_pass "$SVC has no published ports (not reachable from host)"
  else
    log_fail "$SVC has published ports: $ANY_PORT (expected none)"
  fi
done

# ============================================================
# 3. web01 services: HTTP 200, hidden admin panel, XSS injectable
# ============================================================
echo ""
echo "=== Test: web01 services ==="

WEB_HTTP=$(exec_in "$WEB01" python -c \
  "import urllib.request; print(urllib.request.urlopen('http://localhost:80/').read().decode())" 2>&1 || echo "ERR")
if echo "$WEB_HTTP" | grep -qi "TechPulse"; then
  log_pass "web01 HTTP 200 with TechPulse branding"
else
  log_fail "web01 homepage missing TechPulse branding"
fi

ADMIN_CODE=$(exec_in "$WEB01" python -c "
import urllib.request, urllib.error
try:
    r = urllib.request.urlopen('http://localhost:80/admin-portal')
    print(r.getcode())
except urllib.error.HTTPError as e:
    print(e.code)
" 2>&1 || echo "ERR")
if echo "$ADMIN_CODE" | grep -qE "200|403"; then
  log_pass "web01 hidden admin panel exists at /admin-portal ($ADMIN_CODE)"
else
  log_fail "web01 /admin-portal: $ADMIN_CODE (expected 200 or 403)"
fi

XSS_CHECK=$(exec_in "$WEB01" python -c "
import urllib.request, urllib.parse
data = urllib.parse.urlencode({'name': 'tester', 'message': '<script>alert(1)</script>'}).encode()
urllib.request.urlopen(urllib.request.Request('http://localhost:80/guestbook', data=data))
body = urllib.request.urlopen('http://localhost:80/guestbook').read().decode()
print('XSS_FOUND' if '<script>alert(1)</script>' in body else 'XSS_ESCAPED')
" 2>&1 || echo "ERR")
if echo "$XSS_CHECK" | grep -q "XSS_FOUND"; then
  log_pass "web01 guestbook is XSS injectable (unescaped script tag)"
else
  log_fail "web01 guestbook XSS: $XSS_CHECK"
fi

# ============================================================
# 4. app01: SSH accepts expected credentials, HTTP on 8080
# ============================================================
echo ""
echo "=== Test: app01 services ==="

APP01_HTTP=$(exec_in "$WEB01" python -c \
  "import urllib.request; print(urllib.request.urlopen('http://app01:8080/').getcode())" 2>&1 || echo "ERR")
if echo "$APP01_HTTP" | grep -q "200"; then
  log_pass "app01 HTTP responds 200 on port 8080 (via web01 pivot)"
else
  log_fail "app01 HTTP response: $APP01_HTTP (expected 200)"
fi

APP01_SSH=$(exec_in "$WEB01" bash -c \
  "echo '' | timeout 5 bash -c 'cat < /dev/tcp/app01/22' 2>/dev/null | head -1" 2>&1 || echo "")
if echo "$APP01_SSH" | grep -qi "SSH"; then
  log_pass "app01 SSH banner visible from web01"
else
  log_fail "app01 SSH banner not found: $APP01_SSH"
fi

SSH_TEST=$(exec_in "$APP01" bash -c \
  "echo 'TechPulse2024!' | su -c 'echo AUTH_OK' devops 2>&1" || echo "")
if echo "$SSH_TEST" | grep -q "AUTH_OK"; then
  log_pass "app01 user devops authenticates with expected password"
else
  log_fail "app01 credential test: $SSH_TEST"
fi

# ============================================================
# 5. db01: MySQL accepts expected credentials
# ============================================================
echo ""
echo "=== Test: db01 MySQL ==="

MYSQL_TEST=$(exec_in "$DB01" bash -c \
  "mysql -u tp_app -p'Pr0d-DB#Acc3ss!' techpulse_prod -e 'SELECT COUNT(*) AS cnt FROM users;' 2>&1" || echo "ERR")
if echo "$MYSQL_TEST" | grep -q "cnt"; then
  log_pass "db01 MySQL accepts tp_app credentials and users table exists"
else
  log_fail "db01 MySQL test: $MYSQL_TEST"
fi

DC_CREDS=$(exec_in "$DB01" bash -c \
  "mysql -u tp_app -p'Pr0d-DB#Acc3ss!' techpulse_prod -N -e \"SELECT password FROM users WHERE username='dc01admin';\" 2>&1" || echo "ERR")
if echo "$DC_CREDS" | grep -q "Domain@dmin2024!"; then
  log_pass "db01 contains dc01admin plaintext credentials"
else
  log_fail "db01 dc01admin creds not found: $DC_CREDS"
fi

# ============================================================
# 6. dc01: SSH accepts expected credentials, flag readable
# ============================================================
echo ""
echo "=== Test: dc01 SSH and flag ==="

DC01_SSH=$(exec_in "$WEB01" bash -c \
  "echo '' | timeout 5 bash -c 'cat < /dev/tcp/dc01/22' 2>/dev/null | head -1" 2>&1 || echo "")
if echo "$DC01_SSH" | grep -qi "SSH"; then
  log_pass "dc01 SSH banner visible from web01"
else
  log_fail "dc01 SSH banner not found: $DC01_SSH"
fi

DC_AUTH=$(exec_in "$DC01" bash -c \
  "echo 'Domain@dmin2024!' | su -c 'echo AUTH_OK' dc01admin 2>&1" || echo "")
if echo "$DC_AUTH" | grep -q "AUTH_OK"; then
  log_pass "dc01 SSH accepts dc01admin:Domain@dmin2024!"
else
  log_fail "dc01 credential test: $DC_AUTH"
fi

FLAG=$(exec_in "$DC01" bash -c \
  "echo 'Domain@dmin2024!' | su -c 'sudo cat /root/flag.txt' dc01admin 2>&1" || echo "")
if echo "$FLAG" | grep -qi "flag\|FLAG\|{"; then
  log_pass "dc01 flag is readable with admin access"
else
  log_fail "dc01 flag read: $FLAG"
fi

# ============================================================
# Results
# ============================================================
echo ""
echo "========================"
echo "Results: $PASS passed, $FAIL failed"
if [ "$FAIL" -gt 0 ]; then
  echo -e "\nFailures:$ERRORS"
  exit 1
fi
echo "All complex lab smoke tests passed."
