#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TESTS_DIR="$REPO_ROOT/eval/tests"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------
MODE="test"
FAST=false
TEST_FILTER=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --fix)
            MODE="fix"
            shift
            ;;
        --fast)
            FAST=true
            shift
            ;;
        -*)
            echo "Unknown flag: $1" >&2
            echo "Usage: $0 [--fix] [--fast] [test-name]" >&2
            exit 1
            ;;
        *)
            TEST_FILTER="$1"
            shift
            ;;
    esac
done

# ---------------------------------------------------------------------------
# Compile once
# ---------------------------------------------------------------------------
echo "Building rt..."
if ! cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>&1; then
    echo -e "${RED}Build failed${NC}"
    exit 1
fi
export RT_BIN="$REPO_ROOT/target/release/rt"
echo ""

# ---------------------------------------------------------------------------
# Discover tests
# ---------------------------------------------------------------------------
discover_tests() {
    local tests=()
    for test_script in "$TESTS_DIR"/*.sh; do
        [[ -f "$test_script" ]] || continue
        local name
        name="$(basename "$test_script")"

        # Apply --fast filter: skip .llm.sh
        if [[ "$FAST" == true && "$name" == *.llm.sh ]]; then
            continue
        fi

        # Apply name filter
        if [[ -n "$TEST_FILTER" ]]; then
            # Match: exact filename, feature-<name>.sh, feature-<name>.llm.sh, or <name>
            local base="${name%.sh}"
            base="${base%.llm}"
            if [[ "$name" != "$TEST_FILTER" && \
                  "$base" != "$TEST_FILTER" && \
                  "$base" != "feature-$TEST_FILTER" ]]; then
                continue
            fi
        fi

        tests+=("$test_script")
    done
    [[ ${#tests[@]} -gt 0 ]] && printf '%s\n' "${tests[@]}"
}

TESTS=()
while IFS= read -r line; do
    [[ -n "$line" ]] && TESTS+=("$line")
done < <(discover_tests)

if [[ ${#TESTS[@]} -eq 0 ]]; then
    echo "No tests found${TEST_FILTER:+ matching '$TEST_FILTER'}"
    exit 1
fi

# ---------------------------------------------------------------------------
# Test execution helpers
# ---------------------------------------------------------------------------
run_single_test() {
    local script="$1"
    local timeout_secs="$2"
    timeout "$timeout_secs" bash "$script" > /dev/null 2>&1
    return $?
}

run_test() {
    local script="$1"
    local name
    name="$(basename "$script")"

    if [[ "$name" == *.llm.sh ]]; then
        # Majority vote: 3 runs, pass if 2+ succeed
        local pass_count=0
        for _ in 1 2 3; do
            if run_single_test "$script" 300; then
                (( pass_count++ )) || true
            fi
        done
        [[ "$pass_count" -ge 2 ]]
        return $?
    else
        run_single_test "$script" 180
        return $?
    fi
}

# ---------------------------------------------------------------------------
# Run test mode
# ---------------------------------------------------------------------------
run_test_mode() {
    local passed=0
    local failed=0
    local total=${#TESTS[@]}

    for test_script in "${TESTS[@]}"; do
        local name
        name="$(basename "$test_script" .sh)"
        name="${name%.llm}"

        printf "  %-40s " "$name"
        if run_test "$test_script"; then
            echo -e "${GREEN}PASS${NC}"
            (( passed++ )) || true
        else
            echo -e "${RED}FAIL${NC}"
            (( failed++ )) || true
        fi
    done

    echo ""
    if [[ "$failed" -eq 0 ]]; then
        echo -e "${GREEN}$passed/$total passed${NC}"
    else
        echo -e "${RED}$passed/$total passed${NC}"
    fi

    [[ "$failed" -eq 0 ]]
    return $?
}

# ---------------------------------------------------------------------------
# Dispatch
# ---------------------------------------------------------------------------
if [[ "$MODE" == "test" ]]; then
    run_test_mode
    exit $?
fi

# Fix mode placeholder — implemented in Task 3
echo "Fix mode not yet implemented"
exit 1
