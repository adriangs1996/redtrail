# Live Test Runner

## Problem

The eval loop (`eval/loop.sh` + `eval/score.sh`) provides end-to-end testing with scoring, metrics, and regression detection — but it's designed for autonomous improvement loops, not for fast developer feedback. Developers need a lightweight way to run E2E tests during development, and a way to let Claude Code iterate on failing tests automatically.

## Design

### Two Modes

The runner (`scripts/live-test.sh`) operates in two modes:

**Test mode** (default) — run tests and report results:

```bash
./scripts/live-test.sh                # run all tests
./scripts/live-test.sh feature-init   # run one specific test
./scripts/live-test.sh --fast         # skip .llm.sh tests
```

**Fix mode** — Claude Code iterates until tests pass:

```bash
./scripts/live-test.sh --fix                # fix all failing tests
./scripts/live-test.sh --fix feature-init   # fix one specific test
```

### Test Mode

#### Execution flow

1. **Compile once** — `cargo build --release`, export `RT_BIN=target/release/rt`
2. **Discover tests** — glob `eval/tests/*.sh`
   - If a name argument is provided: filter to `eval/tests/feature-<name>.sh` or `eval/tests/<name>.sh`
   - If `--fast`: exclude `*.llm.sh`
3. **Execute each test** (with per-test timeout: 3 min for `.sh`, 5 min for `.llm.sh`):
   - Normal tests (`.sh`): 1 run, PASS/FAIL
   - LLM tests (`.llm.sh`): 3 runs, PASS if 2/3 succeed (majority vote). Note: `score.sh` uses 5 runs / 3 majority for higher confidence in the eval loop; we intentionally use 3/2 here for faster dev feedback.
4. **Output** — per test: name + PASS/FAIL with color. Summary: `X/Y passed`
5. **Exit code** — 0 if all pass, 1 if any fail

#### Contract with existing tests

Tests in `eval/tests/` are modified minimally:

- If `$RT_BIN` is set, use it directly (skip compilation)
- If `$RT_BIN` is not set, compile as before (backward compatible with `score.sh`)

This is ~3 lines of change per test script.

#### What does NOT change

- `eval/score.sh` continues working as-is
- `eval/loop.sh` is not touched
- Tests stay in `eval/tests/` — single source of truth

### Fix Mode

#### Execution flow

1. Compile once, run target test(s)
2. If all pass → `"nothing to fix"`, exit 0
3. Record which tests currently pass (snapshot for regression detection)
4. Initialize `attempts_log = []`
5. **Loop (max 5 iterations):**
   - Invoke Claude Code with prompt containing:
     - Source code of the failing test
     - Stderr/stdout of the failure
     - `attempts_log` — what was already tried and why it failed
     - Instruction: "Make this test pass without breaking the others. Explain your approach before implementing."
   - Claude responds with: explanation of approach + code changes
   - Protect `eval/` directory: `git checkout -- eval/`
   - Compile all tests
   - **If build fails** → revert (`git checkout -- .` + `git clean -fd -- src/`), append to `attempts_log`:
     ```
     {approach: "<what Claude tried>", result: "build failed", error: "<compiler output>"}
     ```
   - Run **all** tests (with timeouts: 3 min for `.sh`, 5 min for `.llm.sh`)
   - **Decision:**
     - **Target passes + no regressions** → commit with message `[live-fix] <test-name>: <summary>`, exit 0
     - **Regression** (a previously-passing test now fails) → revert (`git checkout -- .` + `git clean -fd -- src/`), append to `attempts_log`:
       ```
       {approach: "<what Claude tried>", result: "regression in <test>", diff: "<summary>"}
       ```
     - **Target still failing** (no regression) → do NOT revert (accumulate progress), append to `attempts_log`:
       ```
       {approach: "<what Claude tried>", result: "still failing", output: "<test output>"}
       ```
6. After 5 attempts → report all attempts and exit 1

#### Multi-test fix strategy

When `--fix` is invoked without a test name and multiple tests fail, the runner fixes them **one at a time**, in discovery order. Each test goes through the full fix loop (up to 5 iterations) before moving to the next. This keeps Claude focused on one problem at a time and avoids conflicting changes.

#### Accumulated memory

The `attempts_log` is the key mechanism that prevents Claude from repeating failed approaches. Each entry contains:

- **approach**: what was tried (extracted from Claude's explanation)
- **result**: what happened (regression, still failing, build error)
- **diff** or **output**: evidence of what went wrong

This is passed as context in every subsequent Claude invocation, so each iteration builds on prior knowledge.

#### Protections

- `eval/` directory is protected — `git checkout -- eval/` after each Claude invocation (same as eval loop)
- Revert uses `git checkout -- .` + `git clean -fd -- src/` to cover both modified and newly created files
- No code quality metrics — that's the eval loop's job
- No push — only local commits
- Max 5 iterations — prevents infinite loops

### Test naming convention

Tests follow the existing convention in `eval/tests/`:

- `feature-<name>.sh` — deterministic E2E test
- `feature-<name>.llm.sh` — E2E test involving LLM calls (gets majority vote execution)

The runner accepts either the full filename or just the feature name:

```bash
./scripts/live-test.sh feature-init      # matches eval/tests/feature-init.sh
./scripts/live-test.sh init              # also matches eval/tests/feature-init.sh
```

## Files

| File | Action |
|------|--------|
| `scripts/live-test.sh` | Create — the runner |
| `eval/tests/feature-init.sh` | Modify — accept `$RT_BIN` |
| `eval/tests/feature-kb-query.sh` | Modify — accept `$RT_BIN` |
| `eval/tests/feature-claude-code-extraction.llm.sh` | Modify — accept `$RT_BIN` |
