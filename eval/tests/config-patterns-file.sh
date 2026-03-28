#!/usr/bin/env bash
# Live test: secrets.patterns_file config set/get roundtrip
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_CONFIG="$TMPDIR/config.yaml"

# Default should not have patterns_file
DEFAULT=$("$RT" config 2>/dev/null)
echo "$DEFAULT" | grep -q 'patterns_file' && {
  echo "FAIL: default config should not include patterns_file. Got: $DEFAULT"
  exit 1
}

# Set a patterns file path
"$RT" config set secrets.patterns_file /home/user/.redtrail/patterns.yaml 2>/dev/null
AFTER=$("$RT" config 2>/dev/null)
echo "$AFTER" | grep -q 'patterns.yaml' || {
  echo "FAIL: patterns_file should be set. Got: $AFTER"
  exit 1
}

echo "PASS"
