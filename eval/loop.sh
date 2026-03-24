#!/usr/bin/env bash
# eval/loop.sh — Outer Loop Orchestrator
# Manages the autonomous eval experiment cycle for Redtrail.
# Inspired by karpathy/autoresearch: no scoped goals, just "improve the score."
# Requires bash 5+
set -uo pipefail

# ---------------------------------------------------------------------------
# Resolve paths
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
EVAL_DIR="$SCRIPT_DIR"
GOALS_DIR="$EVAL_DIR/goals"
RESULTS_TSV="$EVAL_DIR/results.tsv"
PROGRAM_MD="$EVAL_DIR/program.md"
SCORE_SH="$EVAL_DIR/score.sh"
LAST_PASSING="$EVAL_DIR/.last_passing"

# ---------------------------------------------------------------------------
# Defaults / CLI parsing
# ---------------------------------------------------------------------------
OPT_MAX_ITERATIONS=""
OPT_TARGET_SCORE=""

usage() {
  cat >&2 <<EOF
Usage: $0 [OPTIONS]

Options:
  --max-iterations N      Stop after N experiments
  --target-score SCORE    Stop when composite score reaches SCORE
  -h, --help              Show this help
EOF
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
  --max-iterations)
    OPT_MAX_ITERATIONS="$2"
    shift 2
    ;;
  --target-score)
    OPT_TARGET_SCORE="$2"
    shift 2
    ;;
  -h | --help)
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
KEPT_COUNT=0
REVERTED_COUNT=0

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

# collect_goals: concatenate all goal files into a single block
collect_goals() {
  local output=""
  for f in "$GOALS_DIR"/*.md; do
    [[ -f "$f" ]] || continue
    local name
    name="$(basename "$f" .md)"
    output="${output}
### ${name}

$(cat "$f")

---
"
  done
  echo "$output"
}

# assemble_prompt EVAL_OUTPUT
# Prints the assembled prompt to stdout — no goal scoping, full freedom.
assemble_prompt() {
  local eval_out="$1"

  local program_content
  program_content="$(cat "$PROGRAM_MD")"

  local all_goals
  all_goals="$(collect_goals)"

  # Last 10 experiment history from results.tsv
  local history=""
  if [[ -f "$RESULTS_TSV" ]]; then
    history="$(tail -10 "$RESULTS_TSV")"
  fi

  cat <<PROMPT
${program_content}

---

## Your Mission

Improve the composite score. You have full freedom to choose what to work on.
Look at the eval results below — find the highest-leverage change you can make
in a single vertical slice (one feature or improvement, touching as many files
as needed).

Priorities (in order):
1. Make a failing test pass (highest impact on score)
2. Improve code quality, reduce latency, reduce LLM calls
3. Implement new capabilities described in the goals

Do NOT repeat approaches that were already tried and reverted (see history below).

---

## All Goals (context — pick what gives the best score improvement)

${all_goals}

---

## Current Eval Results

${eval_out}

---

## Recent Experiment History (last 10)

${history:-"(no history yet)"}
PROMPT
}

# record_result ITERATION DESCRIPTION SCORE PASSED TOTAL STATUS
record_result() {
  local iter="$1"
  local description="$2"
  local score="$3"
  local passed="$4"
  local total="$5"
  local status="$6"

  # Ensure TSV header exists
  if [[ ! -f "$RESULTS_TSV" ]]; then
    printf 'experiment\tdescription\tcomposite_score\ttests_passed\ttests_total\tstatus\n' \
      >"$RESULTS_TSV"
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$iter" "$description" "$score" "$passed" "$total" "$status" \
    >>"$RESULTS_TSV"
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
  echo "  Iterations:  $ITERATION ($KEPT_COUNT kept, $REVERTED_COUNT reverted)"
  echo "  Best score:  $BEST_SCORE"
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
  printf 'experiment\tdescription\tcomposite_score\ttests_passed\ttests_total\tstatus\n' \
    >"$RESULTS_TSV"
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
  # Stopping conditions
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

  # Stuck: too many consecutive reverts with no improvement at all
  if [[ "$CONSECUTIVE_REVERTS" -ge 8 ]]; then
    log "8 consecutive reverts with no improvement. Stopping."
    break
  fi

  ITERATION=$((ITERATION + 1))
  log_section "Iteration $ITERATION"

  # ------------------------------------------------------------------
  # Assemble prompt (unscoped — Claude picks what to work on)
  # ------------------------------------------------------------------
  PROMPT_TEXT="$(assemble_prompt "$EVAL_OUTPUT")"

  # ------------------------------------------------------------------
  # Invoke Claude Code
  # ------------------------------------------------------------------
  log "Invoking Claude Code..."
  CLAUDE_OUTPUT=""
  CLAUDE_OUTPUT="$(echo "$PROMPT_TEXT" | claude -p --dangerously-skip-permissions 2>&1)" || true

  # ------------------------------------------------------------------
  # Protect the sacred eval/ directory
  # ------------------------------------------------------------------
  git checkout -- eval/ 2>/dev/null || true

  # ------------------------------------------------------------------
  # Check for changes
  # ------------------------------------------------------------------
  CHANGED_FILES="$(
    git diff --name-only HEAD 2>/dev/null
    git ls-files --others --exclude-standard 2>/dev/null
  )"
  if [[ -z "$CHANGED_FILES" ]]; then
    log "No changes detected. Skipping eval."
    record_result "$ITERATION" "no changes" "$BEST_SCORE" \
      "$BASELINE_PASSED" "$BASELINE_TOTAL" "skip"
    CONSECUTIVE_REVERTS=$((CONSECUTIVE_REVERTS + 1))
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
    record_result "$ITERATION" "${DESCRIPTION:-build failed}" \
      "$BEST_SCORE" "" "" "revert:build"
    CONSECUTIVE_REVERTS=$((CONSECUTIVE_REVERTS + 1))
    REVERTED_COUNT=$((REVERTED_COUNT + 1))
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
      log "Regression detected ($NEW_REGRESSED tests). Reverting."
      git checkout -- . 2>/dev/null || true
      git clean -fd -- src/ tests/ skills/ 2>/dev/null || true
      record_result "$ITERATION" \
        "${DESCRIPTION:-regression}" "$NEW_SCORE" "$NEW_PASSED" "$NEW_TOTAL" \
        "revert:regression"
      CONSECUTIVE_REVERTS=$((CONSECUTIVE_REVERTS + 1))
      REVERTED_COUNT=$((REVERTED_COUNT + 1))
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
        KEPT_COUNT=$((KEPT_COUNT + 1))

        git add -A
        git commit -m "[eval-loop] ${DESCRIPTION:-improvement}
Score: $BEST_SCORE | Tests: $NEW_PASSED/$NEW_TOTAL"

        record_result "$ITERATION" \
          "${DESCRIPTION:-improvement}" "$NEW_SCORE" "$NEW_PASSED" \
          "$NEW_TOTAL" "keep"
      else
        # No improvement — revert
        log "No score improvement ($NEW_SCORE <= $BEST_SCORE). Reverting."
        git checkout -- . 2>/dev/null || true
        git clean -fd -- src/ tests/ skills/ 2>/dev/null || true
        record_result "$ITERATION" \
          "${DESCRIPTION:-no improvement}" "$NEW_SCORE" "$NEW_PASSED" \
          "$NEW_TOTAL" "revert:no-improvement"
        CONSECUTIVE_REVERTS=$((CONSECUTIVE_REVERTS + 1))
        REVERTED_COUNT=$((REVERTED_COUNT + 1))
      fi
    fi
  fi

done

# trap will call print_summary on exit
