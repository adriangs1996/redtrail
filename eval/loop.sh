#!/usr/bin/env bash
# eval/loop.sh — Outer Loop Orchestrator
# Manages the autonomous eval experiment cycle for Redtrail.
# Requires bash 5+
set -uo pipefail

# ---------------------------------------------------------------------------
# Resolve paths
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
EVAL_DIR="$SCRIPT_DIR"
GOALS_DIR="$EVAL_DIR/goals"
TESTS_DIR="$EVAL_DIR/tests"
RESULTS_TSV="$EVAL_DIR/results.tsv"
PROGRAM_MD="$EVAL_DIR/program.md"
SCORE_SH="$EVAL_DIR/score.sh"
LAST_PASSING="$EVAL_DIR/.last_passing"

# ---------------------------------------------------------------------------
# Defaults / CLI parsing
# ---------------------------------------------------------------------------
OPT_GOAL=""
OPT_MAX_ITERATIONS=""
OPT_TARGET_SCORE=""

usage() {
    cat >&2 <<EOF
Usage: $0 [OPTIONS]

Options:
  --goal GOAL_NAME        Focus on a specific goal (basename without .md)
  --max-iterations N      Stop after N experiments
  --target-score SCORE    Stop when composite score reaches SCORE
  -h, --help              Show this help
EOF
    exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --goal)
            OPT_GOAL="$2"
            shift 2
            ;;
        --max-iterations)
            OPT_MAX_ITERATIONS="$2"
            shift 2
            ;;
        --target-score)
            OPT_TARGET_SCORE="$2"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage
            ;;
    esac
done

# ---------------------------------------------------------------------------
# State
# ---------------------------------------------------------------------------
SESSION_BRANCH="eval/loop-session-$(date +%s)"
ITERATION=0
BEST_SCORE=-999999
CONSECUTIVE_REVERTS=0
WIDENED_SCOPE=0

declare -a COMPLETED_GOALS=()
declare -a SKIPPED_GOALS=()

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

log() {
    echo "[loop] $*"
}

log_section() {
    echo ""
    echo "══════════════════════════════════════════════════════"
    echo "  $*"
    echo "══════════════════════════════════════════════════════"
}

# run_eval: invoke score.sh, optionally with --previous-passing
# Sets global: EVAL_OUTPUT
run_eval() {
    local args=()
    if [[ -f "$LAST_PASSING" ]]; then
        args+=(--previous-passing "$LAST_PASSING")
    fi
    EVAL_OUTPUT="$(bash "$SCORE_SH" "${args[@]}")"
}

# parse_result KEY: extract a value from $EVAL_OUTPUT
parse_result() {
    local key="$1"
    echo "$EVAL_OUTPUT" | grep "^${key}=" | head -1 | cut -d= -f2-
}

# list_goals: print all goal basenames (without .md)
list_goals() {
    for f in "$GOALS_DIR"/*.md; do
        [[ -f "$f" ]] || continue
        basename "$f" .md
    done
}

# is_in_array VAL ARRAY_ELEMENTS...
is_in_array() {
    local val="$1"
    shift
    local elem
    for elem in "$@"; do
        [[ "$elem" == "$val" ]] && return 0
    done
    return 1
}

# goal_test_file GOAL_NAME: return matching test script path (or empty)
goal_test_file() {
    local goal="$1"
    local tf="$TESTS_DIR/${goal}.sh"
    if [[ -f "$tf" ]]; then
        echo "$tf"
    fi
}

# test_passes SCRIPT: returns 0 if the test passes
test_passes() {
    local script="$1"
    bash "$script" > /dev/null 2>&1
}

# select_goal: pick the next goal to work on
# Sets global: CURRENT_GOAL
select_goal() {
    # If --goal specified, always use it (unless completed or skipped)
    if [[ -n "$OPT_GOAL" ]]; then
        if is_in_array "$OPT_GOAL" "${COMPLETED_GOALS[@]+"${COMPLETED_GOALS[@]}"}" \
                       "${SKIPPED_GOALS[@]+"${SKIPPED_GOALS[@]}"}"; then
            CURRENT_GOAL=""
        else
            CURRENT_GOAL="$OPT_GOAL"
        fi
        return
    fi

    # Auto-select: prefer failing goals first
    local failing_goal=""
    local no_test_goal=""
    local goal

    while IFS= read -r goal; do
        # Skip completed or skipped
        if is_in_array "$goal" "${COMPLETED_GOALS[@]+"${COMPLETED_GOALS[@]}"}" \
                       "${SKIPPED_GOALS[@]+"${SKIPPED_GOALS[@]}"}"; then
            continue
        fi

        local test_file
        test_file="$(goal_test_file "$goal")"

        if [[ -n "$test_file" ]]; then
            if ! test_passes "$test_file"; then
                # This test is failing — prefer it
                failing_goal="$goal"
                break
            fi
        else
            # No matching test — candidate for refactor pass
            if [[ -z "$no_test_goal" ]]; then
                no_test_goal="$goal"
            fi
        fi
    done < <(list_goals)

    if [[ -n "$failing_goal" ]]; then
        CURRENT_GOAL="$failing_goal"
    elif [[ -n "$no_test_goal" ]]; then
        CURRENT_GOAL="$no_test_goal"
    else
        CURRENT_GOAL=""
    fi
}

# assemble_prompt GOAL EVAL_OUTPUT WIDENED_SCOPE
# Prints the assembled prompt to stdout
assemble_prompt() {
    local goal="$1"
    local eval_out="$2"
    local widened="$3"

    local goal_file="$GOALS_DIR/${goal}.md"
    local program_content
    program_content="$(cat "$PROGRAM_MD")"
    local goal_content=""
    if [[ -f "$goal_file" ]]; then
        goal_content="$(cat "$goal_file")"
    fi

    # Last 10 experiment history from results.tsv
    local history=""
    if [[ -f "$RESULTS_TSV" ]]; then
        history="$(tail -10 "$RESULTS_TSV")"
    fi

    local widened_instructions=""
    if [[ "$widened" -eq 1 ]]; then
        widened_instructions="$(cat <<'WIDENED'

## WIDENED SCOPE MODE

Previous focused attempts have all been reverted. You are now encouraged to take a broader approach:
- Reconsider the overall design and architecture for this feature
- Look for systemic issues that may be blocking progress
- Consider significant refactors if needed to unlock the goal
- Try a substantially different implementation strategy than previous attempts
WIDENED
)"
    fi

    cat <<PROMPT
${program_content}

---

## Current Goal

${goal_content}

---

## Current Eval Results

${eval_out}

---

## Recent Experiment History (last 10)

${history:-"(no history yet)"}
${widened_instructions}
PROMPT
}

# record_result ITERATION GOAL DESCRIPTION SCORE PASSED TOTAL STATUS
record_result() {
    local iter="$1"
    local goal="$2"
    local description="$3"
    local score="$4"
    local passed="$5"
    local total="$6"
    local status="$7"

    # Ensure TSV header exists
    if [[ ! -f "$RESULTS_TSV" ]]; then
        printf 'experiment\tgoal\tdescription\tcomposite_score\ttests_passed\ttests_total\tstatus\n' \
            > "$RESULTS_TSV"
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$iter" "$goal" "$description" "$score" "$passed" "$total" "$status" \
        >> "$RESULTS_TSV"
}

# extract_summary CLAUDE_OUTPUT: pull SUMMARY: line from Claude's output
extract_summary() {
    local output="$1"
    echo "$output" | grep '^SUMMARY:' | head -1 | sed 's/^SUMMARY:[[:space:]]*//'
}

# print_summary: final exit summary
print_summary() {
    log_section "Session Summary"
    echo "  Branch:      $SESSION_BRANCH"
    echo "  Iterations:  $ITERATION"
    echo "  Best score:  $BEST_SCORE"
    echo "  Completed:   ${COMPLETED_GOALS[*]+"${COMPLETED_GOALS[*]}"}"
    echo "  Skipped:     ${SKIPPED_GOALS[*]+"${SKIPPED_GOALS[*]}"}"
    echo ""
}

# ---------------------------------------------------------------------------
# Trap: always print summary on exit
# ---------------------------------------------------------------------------
trap print_summary EXIT

# ---------------------------------------------------------------------------
# Setup: create session branch
# ---------------------------------------------------------------------------
log_section "Starting eval loop session"
log "Repo: $REPO_ROOT"
log "Branch: $SESSION_BRANCH"

cd "$REPO_ROOT"

git checkout -b "$SESSION_BRANCH"
log "Created branch: $SESSION_BRANCH"

# Ensure results.tsv has a header
if [[ ! -f "$RESULTS_TSV" ]]; then
    printf 'experiment\tgoal\tdescription\tcomposite_score\ttests_passed\ttests_total\tstatus\n' \
        > "$RESULTS_TSV"
fi

# ---------------------------------------------------------------------------
# Baseline eval
# ---------------------------------------------------------------------------
log_section "Running baseline eval"
run_eval
BASELINE_SCORE="$(parse_result COMPOSITE_SCORE)"
BASELINE_PASSED="$(parse_result TESTS_PASSED)"
BASELINE_TOTAL="$(parse_result TESTS_TOTAL)"
BEST_SCORE="${BASELINE_SCORE:--999999}"
log "Baseline: score=$BEST_SCORE  tests=$BASELINE_PASSED/$BASELINE_TOTAL"

# ---------------------------------------------------------------------------
# Main loop
# ---------------------------------------------------------------------------
while true; do
    # ------------------------------------------------------------------
    # Stopping conditions checked at top of loop
    # ------------------------------------------------------------------

    # Max iterations
    if [[ -n "$OPT_MAX_ITERATIONS" && "$ITERATION" -ge "$OPT_MAX_ITERATIONS" ]]; then
        log "Reached max iterations ($OPT_MAX_ITERATIONS). Stopping."
        break
    fi

    # Target score
    if [[ -n "$OPT_TARGET_SCORE" ]]; then
        if awk "BEGIN { exit !($BEST_SCORE >= $OPT_TARGET_SCORE) }"; then
            log "Target score $OPT_TARGET_SCORE reached (current: $BEST_SCORE). Stopping."
            break
        fi
    fi

    # Select goal
    select_goal
    if [[ -z "$CURRENT_GOAL" ]]; then
        log "No eligible goals remaining. Stopping."
        break
    fi

    ITERATION=$(( ITERATION + 1 ))
    log_section "Iteration $ITERATION — goal: $CURRENT_GOAL"

    # ------------------------------------------------------------------
    # Assemble prompt
    # ------------------------------------------------------------------
    PROMPT_TEXT="$(assemble_prompt "$CURRENT_GOAL" "$EVAL_OUTPUT" "$WIDENED_SCOPE")"

    # ------------------------------------------------------------------
    # Invoke Claude Code
    # ------------------------------------------------------------------
    log "Invoking Claude Code..."
    CLAUDE_OUTPUT=""
    CLAUDE_OUTPUT="$(echo "$PROMPT_TEXT" | claude -p --dangerouslySkipPermissions 2>&1)" || true

    # ------------------------------------------------------------------
    # Protect the sacred eval/ directory
    # ------------------------------------------------------------------
    git checkout -- eval/ 2>/dev/null || true

    # ------------------------------------------------------------------
    # Check for changes
    # ------------------------------------------------------------------
    CHANGED_FILES="$(git diff --name-only HEAD 2>/dev/null; git ls-files --others --exclude-standard 2>/dev/null)"
    if [[ -z "$CHANGED_FILES" ]]; then
        log "No changes detected. Skipping eval."
        record_result "$ITERATION" "$CURRENT_GOAL" "no changes" "$BEST_SCORE" \
            "$BASELINE_PASSED" "$BASELINE_TOTAL" "skip"
        continue
    fi

    # ------------------------------------------------------------------
    # Build check
    # ------------------------------------------------------------------
    log "Running cargo build --release..."
    if ! cargo build --release --quiet 2>/dev/null; then
        log "Build failed. Reverting changes."
        git checkout -- . 2>/dev/null || true
        git clean -fd -- src/ tests/ skills/ 2>/dev/null || true
        DESCRIPTION="$(extract_summary "$CLAUDE_OUTPUT")"
        record_result "$ITERATION" "$CURRENT_GOAL" "${DESCRIPTION:-build failed}" \
            "$BEST_SCORE" "" "" "revert:build"
        CONSECUTIVE_REVERTS=$(( CONSECUTIVE_REVERTS + 1 ))
        # stuck detection handled below
    else
        # ------------------------------------------------------------------
        # Run eval
        # ------------------------------------------------------------------
        log "Running eval..."
        run_eval
        NEW_SCORE="$(parse_result COMPOSITE_SCORE)"
        NEW_PASSED="$(parse_result TESTS_PASSED)"
        NEW_TOTAL="$(parse_result TESTS_TOTAL)"
        NEW_REGRESSED="$(parse_result TESTS_REGRESSED)"

        DESCRIPTION="$(extract_summary "$CLAUDE_OUTPUT")"

        # Regression check — instant revert
        if [[ -n "$NEW_REGRESSED" && "$NEW_REGRESSED" -gt 0 ]]; then
            log "Regression detected ($NEW_REGRESSED tests regressed). Reverting."
            git checkout -- . 2>/dev/null || true
            git clean -fd -- src/ tests/ skills/ 2>/dev/null || true
            record_result "$ITERATION" "$CURRENT_GOAL" \
                "${DESCRIPTION:-regression}" "$NEW_SCORE" "$NEW_PASSED" "$NEW_TOTAL" \
                "revert:regression"
            CONSECUTIVE_REVERTS=$(( CONSECUTIVE_REVERTS + 1 ))
        else
            # Score improvement check
            IMPROVED=0
            if awk "BEGIN { exit !(${NEW_SCORE:-0} > $BEST_SCORE) }"; then
                IMPROVED=1
            fi

            if [[ "$IMPROVED" -eq 1 ]]; then
                # Keep: commit the changes
                log "Score improved: $BEST_SCORE → $NEW_SCORE. Keeping."
                BEST_SCORE="$NEW_SCORE"
                CONSECUTIVE_REVERTS=0
                WIDENED_SCOPE=0

                git add -A
                git commit -m "eval: score=${NEW_SCORE} goal=${CURRENT_GOAL} — ${DESCRIPTION:-improvement}"

                record_result "$ITERATION" "$CURRENT_GOAL" \
                    "${DESCRIPTION:-improvement}" "$NEW_SCORE" "$NEW_PASSED" \
                    "$NEW_TOTAL" "keep"

                # Check if goal's matching test now passes
                GOAL_TEST="$(goal_test_file "$CURRENT_GOAL")"
                if [[ -n "$GOAL_TEST" ]] && test_passes "$GOAL_TEST"; then
                    log "Goal $CURRENT_GOAL completed — test now passes."
                    COMPLETED_GOALS+=("$CURRENT_GOAL")
                fi
            else
                # No improvement — revert
                log "No score improvement ($NEW_SCORE <= $BEST_SCORE). Reverting."
                git checkout -- . 2>/dev/null || true
                git clean -fd -- src/ tests/ skills/ 2>/dev/null || true
                record_result "$ITERATION" "$CURRENT_GOAL" \
                    "${DESCRIPTION:-no improvement}" "$NEW_SCORE" "$NEW_PASSED" \
                    "$NEW_TOTAL" "revert:no-improvement"
                CONSECUTIVE_REVERTS=$(( CONSECUTIVE_REVERTS + 1 ))
            fi
        fi
    fi

    # ------------------------------------------------------------------
    # Stuck detection
    # ------------------------------------------------------------------
    if [[ "$CONSECUTIVE_REVERTS" -ge 5 ]]; then
        if [[ "$WIDENED_SCOPE" -eq 0 ]]; then
            log "5 consecutive reverts — switching to widened scope mode."
            WIDENED_SCOPE=1
            CONSECUTIVE_REVERTS=0
        elif [[ "$CONSECUTIVE_REVERTS" -ge 3 ]]; then
            # 3 more reverts in widened mode → skip goal
            log "3 consecutive reverts in widened mode — skipping goal: $CURRENT_GOAL"
            SKIPPED_GOALS+=("$CURRENT_GOAL")
            CONSECUTIVE_REVERTS=0
            WIDENED_SCOPE=0
        fi
    fi

done

# trap will call print_summary on exit
