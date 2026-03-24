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
# Fix mode
# ---------------------------------------------------------------------------
MAX_FIX_ATTEMPTS=5

snapshot_passing_tests() {
    # Run all tests, return list of passing test names
    local passing=()
    for test_script in "$TESTS_DIR"/*.sh; do
        [[ -f "$test_script" ]] || continue
        local name
        name="$(basename "$test_script")"
        if run_test "$test_script"; then
            passing+=("$name")
        fi
    done
    [[ ${#passing[@]} -gt 0 ]] && printf '%s\n' "${passing[@]}"
}

revert_changes() {
    git checkout -- . 2>/dev/null || true
    git clean -fd -- src/ 2>/dev/null || true
}

protect_eval() {
    git checkout -- eval/ 2>/dev/null || true
}

check_regressions() {
    # Args: file with previously passing test names
    local prev_passing_file="$1"
    local regressions=()
    while IFS= read -r test_name; do
        [[ -n "$test_name" ]] || continue
        local test_script="$TESTS_DIR/$test_name"
        [[ -f "$test_script" ]] || continue
        if ! run_test "$test_script"; then
            regressions+=("$test_name")
        fi
    done < "$prev_passing_file"
    [[ ${#regressions[@]} -gt 0 ]] && printf '%s\n' "${regressions[@]}"
}

build_fix_prompt() {
    local test_script="$1"
    local test_output="$2"
    local attempts_log="$3"

    local test_source
    test_source="$(cat "$test_script")"
    local test_name
    test_name="$(basename "$test_script")"

    cat <<__FIX_PROMPT__
You are fixing a failing end-to-end test for the Redtrail CLI tool.

## Failing test: $test_name

\`\`\`bash
$test_source
\`\`\`

## Test output (failure):

\`\`\`
$test_output
\`\`\`

## Previous attempts:

$attempts_log

## Instructions:

1. Explain your approach BEFORE making changes.
2. Make the test pass without breaking other tests.
3. Only modify files in src/ — do NOT modify eval/ or test files.
4. Keep changes minimal and focused.
__FIX_PROMPT__
}

run_fix_mode() {
    local target_tests=("${TESTS[@]}")

    # Step 1: Run target tests to find which ones fail
    echo "Running tests to find failures..."
    local failing_tests=()
    for test_script in "${target_tests[@]}"; do
        local name
        name="$(basename "$test_script")"
        if ! run_test "$test_script"; then
            failing_tests+=("$test_script")
            echo -e "  ${RED}FAIL${NC}: $name"
        else
            echo -e "  ${GREEN}PASS${NC}: $name"
        fi
    done

    if [[ ${#failing_tests[@]} -eq 0 ]]; then
        echo -e "\n${GREEN}All tests pass. Nothing to fix.${NC}"
        return 0
    fi

    # Step 2: Snapshot currently passing tests (for regression detection)
    local passing_snapshot
    passing_snapshot="$(mktemp)"
    trap "rm -f '$passing_snapshot'" RETURN
    snapshot_passing_tests > "$passing_snapshot"

    # Step 3: Fix each failing test one at a time
    for test_script in "${failing_tests[@]}"; do
        local test_name
        test_name="$(basename "$test_script")"
        echo ""
        echo -e "${YELLOW}Fixing: $test_name${NC}"

        local attempts_log=""
        local fixed=false

        # Determine timeout for this test type
        local test_timeout=180
        [[ "$test_name" == *.llm.sh ]] && test_timeout=300

        for attempt in $(seq 1 $MAX_FIX_ATTEMPTS); do
            echo -e "  Attempt $attempt/$MAX_FIX_ATTEMPTS..."

            # Capture current test failure output
            local test_output
            test_output="$(timeout "$test_timeout" bash "$test_script" 2>&1 || true)"

            # Build prompt and invoke Claude Code
            local prompt
            prompt="$(build_fix_prompt "$test_script" "$test_output" "$attempts_log")"
            local claude_output
            claude_output="$(echo "$prompt" | claude -p --dangerously-skip-permissions 2>&1)" || true

            # Protect eval/
            protect_eval

            # Try to build
            if ! cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null; then
                local build_err
                build_err="$(cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>&1 || true)"
                echo -e "  ${RED}Build failed. Reverting.${NC}"
                revert_changes
                attempts_log="${attempts_log}
### Attempt $attempt
- **Approach**: $(echo "$claude_output" | head -20)
- **Result**: build failed
- **Error**: $build_err
"
                continue
            fi

            # Run ALL tests to check for regressions
            local regressions
            regressions="$(check_regressions "$passing_snapshot")"

            if [[ -n "$regressions" ]]; then
                echo -e "  ${RED}Regression detected in: $regressions. Reverting.${NC}"
                local diff_summary
                diff_summary="$(git diff --stat HEAD 2>/dev/null || true)"
                revert_changes
                attempts_log="${attempts_log}
### Attempt $attempt
- **Approach**: $(echo "$claude_output" | head -20)
- **Result**: regression in $regressions
- **Diff**: $diff_summary
"
                continue
            fi

            # Check if target test passes now
            if run_test "$test_script"; then
                echo -e "  ${GREEN}Fixed!${NC}"
                # Commit
                git add -- src/
                git commit -m "[live-fix] $test_name: fixed by Claude Code

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>" 2>/dev/null
                # Update snapshot with newly passing tests
                snapshot_passing_tests > "$passing_snapshot"
                fixed=true
                break
            else
                local new_output
                new_output="$(timeout "$test_timeout" bash "$test_script" 2>&1 || true)"
                echo -e "  ${YELLOW}Still failing. Keeping changes, retrying.${NC}"
                attempts_log="${attempts_log}
### Attempt $attempt
- **Approach**: $(echo "$claude_output" | head -20)
- **Result**: still failing
- **Output**: $new_output
"
            fi
        done

        if [[ "$fixed" != true ]]; then
            echo -e "  ${RED}Could not fix $test_name after $MAX_FIX_ATTEMPTS attempts.${NC}"
            echo ""
            echo "Attempts log:"
            echo "$attempts_log"
        fi
    done
}

# ---------------------------------------------------------------------------
# Dispatch
# ---------------------------------------------------------------------------
if [[ "$MODE" == "test" ]]; then
    run_test_mode
    exit $?
fi

run_fix_mode
exit $?
