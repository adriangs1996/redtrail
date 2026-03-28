#!/usr/bin/env bash
# Live test: secrets.on_detect config set/get roundtrip and validation
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_CONFIG="$TMPDIR/config.yaml"

# Default should be redact
DEFAULT=$("$RT" config 2>/dev/null)
echo "$DEFAULT" | grep -q 'redact' || {
  echo "FAIL: default on_detect should be redact. Got: $DEFAULT"
  exit 1
}

# Set to warn
"$RT" config set secrets.on_detect warn 2>/dev/null
AFTER_WARN=$("$RT" config 2>/dev/null)
echo "$AFTER_WARN" | grep -q 'warn' || {
  echo "FAIL: on_detect should be warn after set. Got: $AFTER_WARN"
  exit 1
}

# Set to block
"$RT" config set secrets.on_detect block 2>/dev/null
AFTER_BLOCK=$("$RT" config 2>/dev/null)
echo "$AFTER_BLOCK" | grep -q 'block' || {
  echo "FAIL: on_detect should be block after set. Got: $AFTER_BLOCK"
  exit 1
}

# Set back to redact
"$RT" config set secrets.on_detect redact 2>/dev/null
AFTER_REDACT=$("$RT" config 2>/dev/null)
echo "$AFTER_REDACT" | grep -q 'redact' || {
  echo "FAIL: on_detect should be redact after set. Got: $AFTER_REDACT"
  exit 1
}

# Invalid value should fail
if "$RT" config set secrets.on_detect delete_everything 2>/dev/null; then
  echo "FAIL: invalid on_detect value should be rejected"
  exit 1
fi

echo "PASS"
