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

FIXTURE="$REPO_ROOT/eval/tests/fixtures/nmap-scan.txt"

# Create a fake nmap that outputs the fixture
mkdir -p "$TMPDIR/bin"
printf '#!/usr/bin/env bash\ncat "%s"\n' "$FIXTURE" > "$TMPDIR/bin/nmap"
chmod +x "$TMPDIR/bin/nmap"
export PATH="$TMPDIR/bin:$PATH"

"$RT" proxy nmap -sV -sC -p- 10.10.10.42 > /dev/null 2>&1 || true

# Verify host fact extracted
HOSTS=$("$RT" sql "SELECT COUNT(*) as count FROM facts WHERE fact_type = 'host'" --json 2>/dev/null)
echo "$HOSTS" | grep -qE '"count": [1-9]' || { echo "FAIL: no host facts extracted"; exit 1; }

# Verify service facts extracted
SERVICES=$("$RT" sql "SELECT COUNT(*) as count FROM facts WHERE fact_type = 'service'" --json 2>/dev/null)
echo "$SERVICES" | grep -qE '"count": [1-9]' || { echo "FAIL: no service facts extracted"; exit 1; }

# Verify relations created
RELS=$("$RT" sql "SELECT COUNT(*) as count FROM relations" --json 2>/dev/null)
echo "$RELS" | grep -qE '"count": [1-9]' || { echo "FAIL: no relations created"; exit 1; }

# Verify extraction status
STATUS=$("$RT" sql "SELECT extraction_status FROM events WHERE tool = 'nmap' LIMIT 1" --json 2>/dev/null)
echo "$STATUS" | grep -q "extracted" || { echo "FAIL: extraction_status should be 'extracted'"; exit 1; }

echo "PASS"
