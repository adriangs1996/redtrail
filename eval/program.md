# Redtrail Eval Loop — Agent Instructions

You are an autonomous agent improving the Redtrail CLI tool. You will receive:
1. These standing instructions
2. A specific goal to work toward
3. Current evaluation results
4. History of recent experiments

## Rules

### What You Can Modify
- `src/**/*.rs` — all Rust source files
- `tests/**/*.rs` — Rust integration tests
- `Cargo.toml` — dependencies (add/modify, never remove without reason)
- `skills/**` — skill definitions and prompts

### What You MUST NOT Modify
- `eval/**` — the evaluation infrastructure is sacred. Do not touch it.
- `.git/**` — do not run git commands

### How to Work
1. Read the goal prompt carefully
2. Read the current eval results to understand what's passing and failing
3. Read the experiment history to avoid repeating failed approaches
4. Plan ONE focused vertical slice — a single feature or improvement that may span multiple files
5. Implement the change
6. Run `cargo build --release` to verify compilation
7. If the build fails, fix the errors before exiting
8. Run `cargo test` to verify existing tests still pass
9. If tests fail and the failure is related to your change, fix it. If unrelated, leave it.

### How to End
When your change is complete and compiles successfully, output a one-line summary of what you changed and why. Format:
```
SUMMARY: <what you did and why>
```
This line will be recorded in the experiment log.

### Quality Standards
- Follow TDD principles: tests verify behavior through public interfaces, not implementation details
- Design deep modules with simple interfaces (A Philosophy of Software Design)
- DRY, YAGNI — no speculative abstractions
- One responsibility per module
- Prefer editing existing files over creating new ones unless the change clearly warrants a new module

### When Working on New Features
- The e2e test script in `eval/tests/` defines the acceptance criteria
- Read the test script to understand exactly what must work
- Implement the minimum code to make the test pass
- You may also add Rust integration tests in `tests/` for faster iteration

### When Working on Refactors
- No test should break. Period.
- The code quality metric will evaluate your changes
- Focus on module cohesion, reduced coupling, clearer interfaces
