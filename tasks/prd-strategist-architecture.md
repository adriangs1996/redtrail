# PRD: Redtrail Agent v2 — Strategist Architecture

## Overview

Replace redtrail's sequential DFS task tree with an intelligence-driven execution
architecture modeled on the cognitive patterns of elite penetration testers,
as defined in `redtrail/PENTESTER_MINDSET.md`.

The current system executes one task at a time in depth-first order using
hardcoded rules (`generate_children()`) to decide what to do next. This
produces scanner-like behavior: uniform coverage, no prioritization, no
adaptation to findings, and ~800 seconds wasted on redundant/useless tasks
in a typical run.

The new architecture introduces three components:

1. **Strategist** — An LLM-based reasoning engine (claude -p, no tools) that
   decides what to do next based on the full Knowledge Base, applying pentester
   thinking patterns from `PENTESTER_MINDSET.md`.
2. **Reactor** — Deterministic fast-path rules that fire instantly on
   high-value events (credential discovery triggers credential spray across
   all services).
3. **Scheduler** — A priority-queue-based concurrent worker pool that executes
   multiple tasks in parallel with adaptive scaling, supporting both active
   tasks and long-running background/passive tasks.

Additionally, a **Task Definition Registry** (inspired by Kubernetes Custom
Resource Definitions) allows the strategist to propose new task types at
runtime. These can be persisted globally for reuse across future engagements,
making redtrail progressively more capable.

## Goals

- Make redtrail behave like an elite pentester: prioritize credential reuse,
  follow attack chains, correlate intelligence across hosts, and avoid
  wasted work.
- Execute independent tasks concurrently with adaptive scaling (2->N workers).
- Support background/passive tasks (traffic capture) that run concurrently
  with active tasks without consuming a worker slot.
- Replace hardcoded child-generation rules with LLM-based strategic reasoning.
- Introduce a CRD-like system where the strategist can define new task types
  at runtime, persist them, and reuse them in future runs.
- Reduce Module 01 completion time from ~28 minutes to under 8 minutes while
  finding all flags.
- Keep the architecture service-agnostic — no hardcoded assumptions about
  specific ports, services, or lab configurations.

## Quality Gates

These commands must pass for every user story:
- `cargo test` — All unit and integration tests pass
- `cargo clippy` — No warnings

## User Stories

### US-001: Task Definition Registry

**Description:** As a redtrail developer, I want a Task Definition Registry (TDR)
that holds both built-in and custom task definitions so that the system can be
extended at runtime without modifying Rust enums.

**Acceptance Criteria:**
- [ ] New struct `TaskDefinition` in `redtrail/src/agent/task_registry.rs` with
      fields: `name`, `description`, `prompt_template` (with `{placeholder}`
      variables), `applicable_when` (conditions like `port:6379`,
      `service:redis`), `expected_output` (which SpecialistResult fields are
      relevant), `default_priority`, `estimated_duration_secs`, `builtin: bool`
- [ ] New struct `TaskRegistry` with methods: `register()`, `get()`, `list()`,
      `is_registered()`
- [ ] All existing `TaskType` variants registered as built-in definitions at
      startup via `TaskRegistry::with_builtins()`
- [ ] `TaskRegistry` is `Serialize`/`Deserialize` for persistence
- [ ] Unit tests: register a definition, retrieve it, list all, verify builtins
      are present
- [ ] The prompt generation currently in `TaskTree::build_task_prompt()` is
      refactored to use `TaskDefinition::prompt_template` with parameter
      substitution

### US-002: Priority System

**Description:** As the scheduler, I want tasks to have a priority score so
that I can execute the highest-value actions first instead of following DFS
order.

**Acceptance Criteria:**
- [ ] New enum `Priority` with variants: `Critical`, `High`, `Medium`, `Low`,
      `Background` in `redtrail/src/agent/task_tree.rs` (or a shared types
      location)
- [ ] `Priority` implements `Ord` so Critical > High > Medium > Low > Background
- [ ] `TaskNode` gains a `priority: Priority` field (default: `Medium`)
- [ ] `TaskNode` gains a `rationale: String` field documenting why this task
      was created
- [ ] `TaskNode` gains a `confidence: f32` field (default: 1.0, range 0.0-1.0)
- [ ] Effective score computed as `priority_weight * confidence` where weights
      are: Critical=100, High=75, Medium=50, Low=25, Background=10
- [ ] New method `TaskTree::next_by_priority()` replaces `next_pending_dfs()`
      — returns the highest effective-score pending task whose dependencies
      are met
- [ ] New method `TaskTree::next_ready_batch(max: usize)` returns up to N
      independent ready tasks sorted by effective score
- [ ] A task is "ready" when: status is Pending AND (no parent OR parent is
      Completed)
- [ ] Unit tests: priority ordering, batch selection, dependency gating

### US-003: Enhanced Knowledge Base

**Description:** As the strategist, I want the Knowledge Base to track
completed and failed task summaries so that I can reason about what has been
tried and what to do next.

**Acceptance Criteria:**
- [ ] New struct `TaskSummary` with fields: `task_name`, `task_type`,
      `target_host`, `duration_secs`, `key_findings` (brief string), `status`
      (completed/failed/timeout), `timestamp`
- [ ] `KnowledgeBase` gains `completed_tasks: Vec<TaskSummary>` and
      `failed_tasks: Vec<TaskSummary>` fields (with `#[serde(default)]` for
      backward compat)
- [ ] New method `KnowledgeBase::add_task_summary()` called after each task
      completion/failure
- [ ] New method `KnowledgeBase::situation_report() -> String` that generates
      a structured text dump of the full KB state suitable for the strategist
      prompt (hosts, ports, services, credentials, flags, completed tasks,
      failed tasks, notes)
- [ ] `situation_report()` omits raw output and keeps summaries concise
      (strategist context window budget)
- [ ] Existing DB serialization in `db.rs` handles the new fields (backward
      compatible via serde defaults)
- [ ] Unit tests: add summaries, generate situation report, verify backward
      compat deserialization

### US-004: Reactor — Event Classification and Credential Spray

**Description:** As the agent, I want a reactor that instantly classifies task
results into intel events and fires deterministic responses (credential spray)
so that high-value findings are acted on without waiting for a strategist call.

**Acceptance Criteria:**
- [ ] New file `redtrail/src/agent/reactor.rs`
- [ ] Enum `IntelEvent` with variants: `CredentialFound`, `AccessGained`,
      `ObjectiveCaptured`, `AttackSurfaceExpanded`, `IntelFound`,
      `VulnerabilityConfirmed`, `TaskFailed`
- [ ] Enum `Urgency` with variants: `InterruptAndReplan`, `InjectAndContinue`,
      `NoteAndContinue`
- [ ] Function `classify_events(result: &SpecialistResult, source_task: &TaskType)
      -> Vec<(IntelEvent, Urgency)>` that inspects result fields and produces
      typed events
- [ ] Function `credential_spray(cred: &Credential, kb: &KnowledgeBase)
      -> Vec<(TaskType, Priority)>` that generates `TryCredentials` tasks for
      every known service, skipping the source service and services where creds
      are already known
- [ ] Function `react(events: &[(IntelEvent, Urgency)], kb: &KnowledgeBase)
      -> ReactorOutput` where `ReactorOutput` contains: `urgent_tasks:
      Vec<(TaskType, Priority)>`, `should_replan: bool`, `notes: Vec<String>`
- [ ] Unit tests: classify credential result -> CredentialFound +
      InterruptAndReplan; classify flag result -> ObjectiveCaptured +
      NoteAndContinue; credential spray generates correct tasks and skips
      source; empty result produces no events

### US-005: Strategist — LLM-Based Strategic Reasoning

**Description:** As the agent, I want a strategist module that calls Claude
(with no tools) to reason about the current KB state and propose prioritized
tasks so that redtrail makes intelligent pentester-level decisions.

**Acceptance Criteria:**
- [ ] New file `redtrail/src/agent/strategist.rs`
- [ ] Struct `Strategist` with method `plan(kb: &KnowledgeBase, tree: &TaskTree,
      registry: &TaskRegistry, running_tasks: &[String])
      -> Result<StrategistPlan, ProbeError>`
- [ ] `StrategistPlan` contains: `assessment: String` (situation summary),
      `tasks: Vec<ProposedTask>`, `new_definitions: Vec<TaskDefinition>`
      (custom CRDs), `is_complete: bool` (strategist says assessment is done)
- [ ] `ProposedTask` contains: `definition_name: String`,
      `params: HashMap<String, String>`, `priority: Priority`,
      `rationale: String`, `confidence: f32` (0.0-1.0 score)
- [ ] The confidence score modulates effective priority: a `High` priority task
      with 0.3 confidence may be scheduled after a `Medium` task with 0.9
      confidence. Formula: `effective_score = priority_weight * confidence`
- [ ] Priority weights: Critical=100, High=75, Medium=50, Low=25, Background=10
- [ ] The strategist prompt is built from `PENTESTER_MINDSET.md` thinking
      patterns + `kb.situation_report()` + running tasks + available task
      definitions from registry
- [ ] Strategist is invoked via `ClaudeExecutor::spawn_claude()` with an empty
      allowed_tools list (no tool execution, pure reasoning)
- [ ] Response is parsed from a JSON block in Claude's output (use
      `===PROBE_STRATEGY===` markers to distinguish from task results)
- [ ] Strategist prompt includes the full list of registered task definitions
      (both builtin and custom) so it can reference them or propose new ones.
      When a proposed custom definition overlaps with a builtin, both are shown
      to the strategist and it decides which to use
- [ ] No hardcoded maximum tree depth — the strategist is trusted to manage
      scope. It can propose arbitrarily deep attack chains if it judges them
      valuable
- [ ] Unit test: verify prompt contains KB situation report, thinking patterns,
      and task definitions; verify parsing of a mock strategist response

### US-006: Concurrent Worker Pool

**Description:** As the scheduler, I want a concurrent worker pool that
executes multiple claude -p tasks in parallel with adaptive scaling so that
independent tasks don't wait in line.

**Acceptance Criteria:**
- [ ] New file `redtrail/src/agent/scheduler.rs`
- [ ] Struct `Scheduler` with configurable `initial_concurrency: usize`
      (default 2) and `max_concurrency: usize` (default 4)
- [ ] Uses `tokio::task::JoinSet` to manage concurrent task executions
- [ ] Method `fill_slots(tree: &TaskTree, executor: &ClaudeExecutor)` spawns
      tasks up to current concurrency limit from priority queue
- [ ] Method `wait_next() -> (u32, Result<TaskResult, ProbeError>)` waits for
      any running task to complete
- [ ] Adaptive scaling: after initial recon phase completes (all depth-0 and
      depth-1 tasks done), scale from `initial_concurrency` to
      `max_concurrency`
- [ ] Method `running_count() -> usize` and `has_capacity() -> bool`
- [ ] `ClaudeExecutor` implements `Clone` (or is wrapped in `Arc`) so it can
      be shared across spawned tasks
- [ ] Integration test: spawn 3 mock tasks concurrently, verify all complete,
      verify results are collected

### US-007: Background / Passive Task Support

**Description:** As the scheduler, I want to support long-running passive tasks
(like traffic capture) that run concurrently in the background without
consuming an active worker slot, so that the "Passive While Active" pentester
pattern is supported.

**Acceptance Criteria:**
- [ ] `TaskNode` gains an `is_background: bool` field (default false)
- [ ] Tasks with `priority: Background` are automatically marked as background
- [ ] Background tasks do NOT count against the active worker concurrency limit
      (they run in a separate slot pool)
- [ ] Maximum background task count is configurable (default: 2)
- [ ] Background task results are checked periodically (every time an active
      task completes) — if a background task has finished, its results are
      merged into KB and reactor events are fired
- [ ] Background tasks have their own timeout (default: 120s for passive
      capture) independent of active task timeout
- [ ] When a background task produces credentials, the reactor fires credential
      spray immediately (same as active tasks)
- [ ] The strategist can propose background tasks by setting
      `priority: "background"` in its plan
- [ ] Background tasks support **interim results**: the executor streams output
      and checks for `===PROBE_RESULT===` markers periodically (not just at
      completion). When an interim result is found mid-execution, it is
      immediately merged into KB and the reactor fires. This is critical because
      a traffic capture might sniff credentials at second 15 that should
      redirect the strategist's plan — waiting until the full 120s timeout
      wastes the intelligence
- [ ] Interim result flow: background stdout stream -> detect markers ->
      parse partial SpecialistResult -> merge into KB -> fire reactor ->
      if InterruptAndReplan, call strategist while background task continues
- [ ] A background task can produce multiple interim results during its
      lifetime (e.g., sniffing FTP creds at t=15, HTTP basic auth at t=30,
      API key at t=45)
- [ ] Unit tests: background tasks don't consume active slots, background
      results trigger reactor, background timeout is independent, interim
      results are parsed and merged before task completion

### US-008: Strategist Inflection Points

**Description:** As the orchestrator, I want the strategist to be called at
specific inflection points (not after every task) so that strategic reasoning
happens when it matters without excessive overhead.

**Acceptance Criteria:**
- [ ] Enum `StrategistTrigger` with variants: `ReconComplete` (all initial port
      scans done), `HighValueDiscovery` (reactor returned `InterruptAndReplan`),
      `WaveComplete` (all running tasks finished and queue is empty), `Stuck`
      (queue empty but objectives remain), `RepeatedFailure` (3+ consecutive
      failures on same host)
- [ ] Function `should_call_strategist(trigger: &StrategistTrigger,
      last_call_elapsed: Duration, tasks_since_last_call: u32) -> bool` with
      debounce logic: don't re-call within 30 seconds of last call unless
      trigger is `HighValueDiscovery`
- [ ] The main orchestrator loop integrates trigger detection: after each task
      completion, check reactor urgency -> if `InterruptAndReplan`, trigger
      strategist; after filling worker slots fails (empty queue), check if
      `WaveComplete` or `Stuck`
- [ ] `ReconComplete` detection: all tasks at depth 0 and depth 1 are
      completed or failed
- [ ] Unit tests: trigger detection for each variant, debounce logic

### US-009: Custom Task Definition Persistence

**Description:** As a redtrail operator, I want custom task definitions proposed
by the strategist to be auto-registered for the current run and optionally
persisted globally so that redtrail gets smarter over time.

**Acceptance Criteria:**
- [ ] When the strategist proposes a `new_definitions` entry, it is immediately
      registered in the in-memory `TaskRegistry` for the current run
- [ ] Each proposed definition includes a `reusable: bool` flag set by the
      strategist (true = useful across engagements, false = session-specific)
- [ ] Reusable definitions are saved to `~/.redtrail/task-definitions.json` after
      the run completes (append, don't overwrite existing)
- [ ] On startup, `TaskRegistry::with_builtins()` also loads from
      `~/.redtrail/task-definitions.json` if it exists
- [ ] The `KnowledgeBase` stores references to custom definitions used in the
      current session (for session replay/review)
- [ ] When a custom definition has the same `applicable_when` conditions as a
      builtin, both are preserved in the registry. The strategist sees both
      and decides which to use based on context. No automatic merging or
      override.
- [ ] A `redtrail definitions list` CLI subcommand shows all registered
      definitions (builtin + custom) with their source
- [ ] A `redtrail definitions remove <name>` CLI subcommand removes a persisted
      custom definition, with a confirmation prompt ("Remove 'RedisEnum'
      globally? [y/N]")
- [ ] Unit tests: persist a definition, reload it, verify it's available;
      verify session-specific definitions are NOT persisted

### US-010: Orchestrator Rewire — Main Execution Loop

**Description:** As the agent, I want the orchestrator to use the new
strategist + reactor + scheduler loop instead of the sequential DFS so that
redtrail operates like an elite pentester.

**Acceptance Criteria:**
- [ ] Remove `TaskTree::generate_children()` and
      `TaskTree::generate_retroactive_tasks()` methods
- [ ] Remove `TaskTree::next_pending_dfs()` method
- [ ] Remove the sequential `TaskTree::run()` loop
- [ ] New method `TaskTree::run_with_strategy()` that implements the full loop:
  1. Seed with PingSweep (always first, hardcoded)
  2. After PingSweep: generate PortScan tasks for discovered hosts
     (deterministic, no LLM needed), skip obvious non-targets (network
     address .0, common gateway .1, self-IP detection)
  3. Start background traffic capture concurrently (passive recon)
  4. Execute port scans concurrently via scheduler
  5. On recon complete -> call strategist for attack plan
  6. Execute strategist plan via scheduler + reactor
  7. On each task completion: merge KB -> classify events via reactor ->
     if `InterruptAndReplan` call strategist -> else fill worker slots from
     priority queue
  8. On wave complete -> call strategist for next wave
  9. Terminate when: strategist says `is_complete: true` OR queue empty + no
     running tasks + strategist confirms done OR max time reached
- [ ] Verbose mode (`--verbose`) streams Claude output for ALL concurrent
      workers with worker-id prefix: `[WORKER-1][CLAUDE] ...`
- [ ] Progress display shows concurrent status:
      `[SCHEDULER] 3 running (1 bg), 5 queued, 12 completed, 1 failed`
- [ ] The `Orchestrator` struct in `orchestrator.rs` creates `Strategist`,
      `Reactor`, `Scheduler`, and `TaskRegistry` and wires them together
- [ ] The existing `ExecutionMode::Orchestrator` path in `orchestrator.rs`
      delegates to `run_with_strategy()`

### US-011: CLI Integration

**Description:** As a redtrail operator, I want CLI flags to control the new
architecture so that I can tune concurrency and behavior.

**Acceptance Criteria:**
- [ ] New flag `--concurrency <N>` (alias `-j`) sets max concurrent workers
      (default: 4)
- [ ] The adaptive scaling uses `min(2, N)` as initial and `N` as max
- [ ] New flag `--strategy-timeout <secs>` sets max seconds for a strategist
      call (default: 60)
- [ ] New flag `--task-timeout <secs>` sets max seconds per executor task
      (default: 300)
- [ ] Existing `--verbose` flag works with concurrent output (worker-id
      prefixed)
- [ ] `redtrail scan --hosts 172.20.1.0/24 -j 4 --verbose` runs with 4 max
      workers and streaming output
- [ ] `redtrail definitions list` and `redtrail definitions remove <name>`
      subcommands work
- [ ] Help text documents all new flags

### US-012: Integration Test — Module 01

**Description:** As a redtrail developer, I want an integration test that runs the
new architecture against Module 01 so that we validate it finds all flags
faster than the DFS baseline.

**Acceptance Criteria:**
- [ ] Test script or binary that: starts Module 01 lab, runs
      `redtrail scan --hosts 172.20.1.0/24 -j 4`, verifies all 4 flags are found:
  - `FLAG{w3b_s3rv3r_3num3r4t3d}` (web server enumeration)
  - `FLAG{ftp_cr3d3nt14ls_sn1ff3d}` (FTP via sniffed or discovered creds)
  - `FLAG{su1d_b1n4ry_pr1v3sc}` (SUID binary privesc on SSH server)
  - `FLAG{d4t4b4s3_3xf1ltr4t3d}` (database exfiltration — bonus flag)
- [ ] The FTP flag must be obtained through the legitimate attack chain:
      credentials discovered (via sniffing, DB dump, or other intel) THEN
      used to access FTP — not via direct `docker exec` on the container
- [ ] Test verifies background traffic capture was launched (check logs for
      background task with SniffTraffic or TrafficCapture)
- [ ] Test captures total wall-clock time and logs it
- [ ] Test verifies strategist was called at least once (check logs for
      `[STRATEGIST]` prefix)
- [ ] Test verifies concurrent execution occurred (check logs for multiple
      `[WORKER-N]` prefixes)
- [ ] Test verifies credential spray occurred after credential discovery
      (check for `TryCredentials` tasks in tree)
- [ ] Baseline comparison: document DFS time (~28 min) vs new architecture
      time in test output

## Functional Requirements

- FR-01: The strategist must receive the full KB situation report including
  discovered hosts, credentials, flags, completed tasks, failed tasks, and
  notes.
- FR-02: The strategist must receive the list of all registered task
  definitions (builtin + custom) so it can reference or extend them.
- FR-03: The strategist prompt must encode the thinking patterns from
  `redtrail/PENTESTER_MINDSET.md`: "Use What You Have", "Go Inside Before Going
  Wider", "Cheapest Test First", "Connect the Dots", "Passive While Active",
  "Know When to Stop", "Reassess on Every Credential", "Map Before You
  Attack", "Identify the Stack Before Testing".
- FR-04: The reactor must fire credential spray within the same loop iteration
  as the credential discovery — no waiting for the next strategist call.
- FR-05: The scheduler must never execute two tasks targeting the same host AND
  same port concurrently (avoid race conditions on shared services).
- FR-06: Task definitions must support parameterized prompt templates with
  `{host}`, `{port}`, `{url}`, `{credentials}`, and arbitrary custom
  placeholders.
- FR-07: The strategist response must be valid JSON parseable into
  `StrategistPlan`. If parsing fails, log the error and fall back to
  reactor-only task generation for that cycle.
- FR-08: Custom task definitions proposed by the strategist must include:
  `name`, `description`, `prompt_template`, `applicable_when` conditions,
  `default_priority`, and `reusable` flag.
- FR-09: The initial recon phase (PingSweep + PortScans) must be deterministic
  (no strategist call needed) to avoid cold-start latency.
- FR-10: Self-IP detection: redtrail must identify the toolbox container's own IP
  and exclude it from port scanning targets.
- FR-11: Background/passive tasks (traffic capture) must start during the recon
  phase and run concurrently with active tasks. Their results must be checked
  after each active task completion and feed into the reactor.
- FR-12: The authorization preamble (CTF lab context, docker exec pattern) must
  be included in ALL task prompts — both strategist reasoning calls and
  executor task calls.
- FR-13: Background tasks must support interim result emission via
  `===PROBE_RESULT===` markers detected during streaming. Each interim result
  triggers KB merge + reactor evaluation immediately, without waiting for task
  completion.
- FR-14: The strategist must include a confidence score (0.0-1.0) for each
  proposed task. The scheduler uses `priority_weight * confidence` as the
  effective score for ordering.
- FR-15: There is no hardcoded maximum tree depth. The strategist manages scope
  and decides when to stop pursuing a chain. This is enforced by the
  termination condition: strategist sets `is_complete: true`.
- FR-16: When overlapping task definitions exist (builtin + custom with similar
  applicability), both are presented to the strategist in context. The
  strategist chooses which to use. No automatic merging or precedence rules.
- FR-17: The `redtrail definitions remove` command must prompt for confirmation
  before deleting a globally persisted definition.

## Non-Goals

- **Multi-model strategist**: Using a different/cheaper LLM for the strategist.
  We use the same `claude -p` for now.
- **Distributed execution**: Running workers on multiple machines. All workers
  run locally.
- **Real-time collaboration**: No operator-in-the-loop during execution. The
  strategist is autonomous.
- **CVE database integration**: The strategist reasons from service versions
  and general knowledge, not a live CVE feed.
- **Persistent attack chains**: Recording multi-step attack chains as reusable
  playbooks (future enhancement).
- **Rollback/undo**: No mechanism to undo actions taken by executor tasks.
- **Human approval for custom definitions**: Custom definitions are
  auto-registered for the current run. Global persistence happens
  automatically for reusable definitions (approval flow is a future
  enhancement).

## Technical Considerations

- **KB sharing model**: Each spawned worker task receives a snapshot of the KB
  at launch time (for prompt building). Results are merged back sequentially
  in the main loop when a task completes. No concurrent writes to KB.
- **ClaudeExecutor cloning**: The executor needs `Clone` or `Arc` wrapping to
  share across spawned tokio tasks. Since it only holds `timeout_secs: u64`
  and `verbose: bool`, `Clone` is trivial.
- **Strategist context budget**: The `situation_report()` must be concise.
  Summarize completed tasks (name + key finding + duration), don't include
  raw output. Target <4000 tokens for the KB dump.
- **Graceful degradation**: If the strategist call fails (timeout, parse error),
  the system falls back to reactor-only behavior. The reactor's credential
  spray + basic priority rules are enough to make progress.
- **Task deduplication**: Before adding strategist-proposed tasks to the queue,
  check `TaskTree::has_task_for()` to avoid duplicates.
- **Database migration**: The `sessions` table in `db.rs` stores KB as JSON in
  the `knowledge` column. New KB fields use `#[serde(default)]` for backward
  compatibility with existing session data.
- **Task Definition file format**: `~/.redtrail/task-definitions.json` is a JSON
  array of `TaskDefinition` objects. Simple append-on-save, full rewrite
  (file is small).
- **Background task implementation**: Background tasks use a separate
  `JoinSet` in the scheduler. They are checked for completion opportunistically
  (not blocking). If a background task is still running when the scan ends,
  it is killed and partial results are merged.
- **Strategist markers**: Use `===PROBE_STRATEGY===` markers (distinct from
  `===PROBE_RESULT===`) so the executor parser can distinguish strategist
  responses from task results.

## Success Metrics

- All 4 Module 01 flags found (3 required + 1 bonus) in under 8 minutes
  (vs ~28 minute DFS baseline)
- FTP flag obtained through legitimate credential discovery chain (sniffing or
  DB dump), not via direct container access
- Background traffic capture runs concurrently during active enumeration
- Strategist called 2-4 times per engagement (not after every task)
- Credential spray fires within 1 second of credential discovery
- At least 2 concurrent workers active during enumeration phase
- Zero wasted tasks on self-IP or network/gateway addresses
- Custom task definitions persist and load correctly across runs

## Resolved Decisions

1. **Confidence scoring**: YES. The strategist provides a 0.0-1.0 confidence
   score per task. Effective priority = `priority_weight * confidence`. This
   means a high-priority but speculative task (High, 0.3 = 22.5) ranks below
   a solid medium-priority task (Medium, 0.9 = 45). See US-002, US-005, FR-14.

2. **Maximum tree depth**: NO hardcoded limit. The strategist is trusted to
   manage scope. It decides when a chain is worth pursuing deeper and when to
   cut losses. Termination is controlled by `is_complete` flag, not depth
   limits. See FR-15.

3. **Overlapping definitions**: Show BOTH builtin and custom definitions to
   the strategist and let it decide which to use based on context. No
   automatic merging or precedence. See US-009, FR-16.

4. **CLI confirmation for removal**: YES. `redtrail definitions remove <name>`
   prompts for confirmation before deleting a globally persisted definition.
   See US-009, FR-17.

5. **Interim results from background tasks**: YES, and this is critical.
   Background tasks (traffic capture) can emit `===PROBE_RESULT===` markers
   mid-execution. Each interim result is immediately parsed, merged into KB,
   and fed through the reactor. This means sniffed credentials at t=15 can
   redirect the strategist's plan while the capture continues running. A
   single background task can produce multiple interim results over its
   lifetime. See US-007, FR-13.
