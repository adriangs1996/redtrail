#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TESTS_DIR="$REPO_ROOT/eval/tests"
EVAL_DIR="$REPO_ROOT/eval"
IMAGE_NAME="redtrail-test"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

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
# Build binary and Docker image
# ---------------------------------------------------------------------------
echo "Building Docker image (includes Rust compilation)..."
if ! docker build -t "$IMAGE_NAME" -f "$EVAL_DIR/Dockerfile" "$REPO_ROOT" 2>&1 | tail -5; then
    echo -e "${RED}Docker build failed${NC}"
    exit 1
fi
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
# Test execution — each test runs in an isolated Docker container
# ---------------------------------------------------------------------------
run_single_test() {
    local script="$1"
    local timeout_secs="$2"
    local name
    name="$(basename "$script")"

    # Run the test script inside Docker with a TTY (-t) for PTY support
    timeout "$timeout_secs" docker run --rm -t "$IMAGE_NAME" "/tests/$name" 2>&1
    return $?
}

run_test() {
    local script="$1"
    local name
    name="$(basename "$script")"

    if [[ "$name" == *.llm.sh ]]; then
        local pass_count=0
        for _ in 1 2 3; do
            if run_single_test "$script" 300 > /dev/null 2>&1; then
                (( pass_count++ )) || true
            fi
        done
        [[ "$pass_count" -ge 2 ]]
        return $?
    else
        run_single_test "$script" 180 > /dev/null 2>&1
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
            # Show output on failure for debugging
            local failname
            failname="$(basename "$test_script")"
            echo -e "    ${YELLOW}Output:${NC}"
            timeout 180 docker run --rm -t "$IMAGE_NAME" "/tests/$failname" 2>&1 | sed 's/^/    /' || true
            echo ""
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
if [[ "$MODE" == "fix" ]]; then
    echo -e "${YELLOW}Fix mode not yet implemented for Docker-based tests${NC}"
    exit 1
fi

run_test_mode
exit $?
