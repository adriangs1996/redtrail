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

FIXTURE="$REPO_ROOT/eval/tests/fixtures/gobuster-scan.txt"

mkdir -p "$TMPDIR/bin"
printf '#!/usr/bin/env bash\ncat "%s"\n' "$FIXTURE" > "$TMPDIR/bin/gobuster"
chmod +x "$TMPDIR/bin/gobuster"
export PATH="$TMPDIR/bin:$PATH"

"$RT" proxy gobuster dir -u http://10.10.10.42 -w wordlist.txt > /dev/null 2>&1 || true

PATHS=$("$RT" sql "SELECT COUNT(*) as count FROM facts WHERE fact_type = 'web_path'" --json 2>/dev/null)
echo "$PATHS" | grep -qE '"count": [1-9]' || { echo "FAIL: no web_path facts extracted"; exit 1; }

echo "PASS"
