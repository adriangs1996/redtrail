#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# Collect all Rust source files into a single context block
SRC_LISTING=""
for f in $(find "$REPO_ROOT/src" -name '*.rs' | sort); do
    REL=$(echo "$f" | sed "s|$REPO_ROOT/||")
    SRC_LISTING="$SRC_LISTING
--- $REL ---
$(cat "$f")
"
done

# Load skill principles as rubric
DESIGN_SKILL=$(cat "$REPO_ROOT/.claude/skills/design-an-interface/SKILL.md" 2>/dev/null || echo "")
TDD_SKILL=$(cat "$REPO_ROOT/.claude/skills/tdd/SKILL.md" 2>/dev/null || echo "")
TDD_DEEP_MODULES=$(cat "$REPO_ROOT/.claude/skills/tdd/deep-modules.md" 2>/dev/null || echo "")
TDD_INTERFACE=$(cat "$REPO_ROOT/.claude/skills/tdd/interface-design.md" 2>/dev/null || echo "")

# Judge prompt — uses /design-an-interface and /tdd skills as scoring rubric
JUDGE_PROMPT="You are a code quality judge for a Rust CLI project called Redtrail.

You MUST use the following skill definitions as your scoring rubric. These are the project's own standards — score the codebase against THESE principles, not generic best practices.

## Rubric: /design-an-interface skill

$DESIGN_SKILL

## Rubric: /tdd skill

$TDD_SKILL

## Rubric: Deep Modules (from /tdd)

$TDD_DEEP_MODULES

## Rubric: Interface Design for Testability (from /tdd)

$TDD_INTERFACE

---

Score the codebase on these dimensions (0-10 each), applying the rubric above:

1. **Module depth**: Are modules deep (small interface, rich implementation) or shallow (large interface, thin pass-through)? Score based on the deep-modules rubric.
2. **Interface design**: Are public APIs minimal, general-purpose, and hiding complexity? Do they accept dependencies rather than create them? Do they return results rather than produce side effects? Score based on the design-an-interface evaluation criteria and interface-design-for-testability rubric.
3. **Coupling**: Can you change one module without rippling to others? Do modules depend on interfaces, not implementations?
4. **Test quality**: Do tests verify behavior through public interfaces, not implementation details? Would tests survive an internal refactor? Are tests written as vertical slices (one behavior per test), not horizontal bulk? Score based on the TDD skill philosophy.
5. **Simplicity**: Is the code DRY and YAGNI? No speculative abstractions, no dead code, no shallow wrapper modules?

Output ONLY a JSON object with these exact keys and integer values:
{\"module_depth\": N, \"interface_design\": N, \"coupling\": N, \"test_quality\": N, \"simplicity\": N, \"total\": N}

Where total = sum of all five scores (0-50).

THE CODEBASE:
$SRC_LISTING"

# Invoke Claude Code as judge
RESULT=$(echo "$JUDGE_PROMPT" | claude -p --model sonnet 2>/dev/null)

# Extract total score
TOTAL=$(echo "$RESULT" | grep -o '"total":[[:space:]]*[0-9]*' | grep -o '[0-9]*$')

if [[ -z "$TOTAL" ]]; then
    echo "0"
    exit 0
fi

echo "$TOTAL"
