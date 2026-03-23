#!/usr/bin/env bash
# eval/score.sh — Test and Metric Orchestrator
# Requires bash 5+
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EVAL_DIR="$SCRIPT_DIR"
TESTS_DIR="$EVAL_DIR/tests"
METRICS_DIR="$EVAL_DIR/metrics"
WEIGHTS_FILE="$EVAL_DIR/weights.env"
LAST_PASSING_FILE="$EVAL_DIR/.last_passing"

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------
PREVIOUS_PASSING_FILE=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --previous-passing)
            PREVIOUS_PASSING_FILE="$2"
            shift 2
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 1
            ;;
    esac
done

# ---------------------------------------------------------------------------
# Load weights
# ---------------------------------------------------------------------------
if [[ -f "$WEIGHTS_FILE" ]]; then
    # shellcheck source=/dev/null
    source "$WEIGHTS_FILE"
fi

WEIGHT_TEST="${WEIGHT_TEST:-10}"
WEIGHT_QUALITY="${WEIGHT_QUALITY:-1}"

# ---------------------------------------------------------------------------
# Load previous-passing set (for regression detection)
# ---------------------------------------------------------------------------
declare -A PREV_PASSING
if [[ -n "$PREVIOUS_PASSING_FILE" && -f "$PREVIOUS_PASSING_FILE" ]]; then
    while IFS= read -r line; do
        [[ -n "$line" ]] && PREV_PASSING["$line"]=1
    done < "$PREVIOUS_PASSING_FILE"
fi

# ---------------------------------------------------------------------------
# Run tests
# ---------------------------------------------------------------------------
TESTS_PASSED=0
TESTS_TOTAL=0
TESTS_REGRESSED=0
declare -a PASSING_TESTS=()

run_test_once() {
    local script="$1"
    bash "$script" > /dev/null 2>&1
    return $?
}

run_test_with_mode() {
    # Run 5 times; pass if 3+ pass (majority mode)
    local script="$1"
    local pass_count=0
    for _ in 1 2 3 4 5; do
        if run_test_once "$script"; then
            (( pass_count++ )) || true
        fi
    done
    [[ "$pass_count" -ge 3 ]]
    return $?
}

if [[ -d "$TESTS_DIR" ]]; then
    for test_script in "$TESTS_DIR"/*.sh; do
        [[ -f "$test_script" ]] || continue

        test_name="$(basename "$test_script")"
        (( TESTS_TOTAL++ )) || true

        passed=0
        if [[ "$test_name" == *.llm.sh ]]; then
            if run_test_with_mode "$test_script"; then
                passed=1
            fi
        else
            if run_test_once "$test_script"; then
                passed=1
            fi
        fi

        if [[ "$passed" -eq 1 ]]; then
            (( TESTS_PASSED++ )) || true
            PASSING_TESTS+=("$test_name")
        else
            # Regression check: was this test passing before?
            if [[ -v PREV_PASSING["$test_name"] ]]; then
                (( TESTS_REGRESSED++ )) || true
            fi
        fi
    done
fi

# ---------------------------------------------------------------------------
# Write .last_passing
# ---------------------------------------------------------------------------
{
    for t in "${PASSING_TESTS[@]+"${PASSING_TESTS[@]}"}"; do
        echo "$t"
    done
} > "$LAST_PASSING_FILE"

# ---------------------------------------------------------------------------
# Run metrics
# ---------------------------------------------------------------------------
declare -A METRIC_VALUES
declare -A METRIC_OUTPUT_NAMES

metric_weight_var() {
    # Convert metric filename (without .sh) to weight var name
    # e.g. code-quality -> WEIGHT_CODE_QUALITY
    local name="$1"
    local var
    var="WEIGHT_$(echo "$name" | tr '[:lower:]-' '[:upper:]_')"
    echo "$var"
}

COMPOSITE_SCORE=0

if [[ -d "$METRICS_DIR" ]]; then
    for metric_script in "$METRICS_DIR"/*.sh; do
        [[ -f "$metric_script" ]] || continue

        metric_filename="$(basename "$metric_script" .sh)"
        metric_value="$(bash "$metric_script" 2>/dev/null || echo "0")"

        # Sanitize: ensure it's a number (integer or float)
        if ! [[ "$metric_value" =~ ^-?[0-9]+(\.[0-9]+)?$ ]]; then
            metric_value=0
        fi

        METRIC_VALUES["$metric_filename"]="$metric_value"

        # Look up specific weight; fall back to WEIGHT_QUALITY
        weight_var="$(metric_weight_var "$metric_filename")"
        weight="${!weight_var:-$WEIGHT_QUALITY}"

        # Accumulate weighted metric into composite score
        COMPOSITE_SCORE=$(awk "BEGIN { printf \"%.6g\", $COMPOSITE_SCORE + ($metric_value * $weight) }")

        # Build output key name: METRIC_CODE_QUALITY
        local_output_key="METRIC_$(echo "$metric_filename" | tr '[:lower:]-' '[:upper:]_')"
        METRIC_OUTPUT_NAMES["$metric_filename"]="$local_output_key"
    done
fi

# ---------------------------------------------------------------------------
# Add test score to composite
# ---------------------------------------------------------------------------
COMPOSITE_SCORE=$(awk "BEGIN { printf \"%.6g\", $COMPOSITE_SCORE + ($TESTS_PASSED * $WEIGHT_TEST) }")

# ---------------------------------------------------------------------------
# Output results
# ---------------------------------------------------------------------------
echo "TESTS_PASSED=$TESTS_PASSED"
echo "TESTS_TOTAL=$TESTS_TOTAL"
echo "TESTS_REGRESSED=$TESTS_REGRESSED"

for metric_filename in "${!METRIC_VALUES[@]}"; do
    output_key="${METRIC_OUTPUT_NAMES[$metric_filename]}"
    echo "${output_key}=${METRIC_VALUES[$metric_filename]}"
done

echo "COMPOSITE_SCORE=$COMPOSITE_SCORE"
