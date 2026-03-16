# Redtrail Integration Testing Plan

## Goal

Prove that Redtrail's deductive reasoning pipeline works correctly end-to-end — not just individual components in isolation, but the **wiring between them**. Every test should call **real production code**, not replicas.

## Architecture Summary (for test authors)

```
Strategist (LLM) → StrategistPlan → Driver.call_strategist_and_enqueue()
                                          ↓
                                    TaskTree (priority queue)
                                          ↓
                              Scheduler.fill_slots() → ClaudeExecutor (spawns `claude -p`)
                                          ↓
                                    TaskResult (SpecialistResult)
                                          ↓
                              Driver.handle_completion()
                                    ├── merge_task_result() → KB updated
                                    ├── apply_reactor()
                                    │     ├── extract_probe_result() → hypothesis.probes updated
                                    │     ├── classify_events() → IntelEvents
                                    │     ├── react() → ReactorOutput (urgent_tasks, cancel_ids)
                                    │     ├── tree.cancel_by_hypothesis() (refuted)
                                    │     └── tree.add_root() (urgent tasks injected)
                                    └── goal.check_criteria() → session_complete
```

**The blocker**: `ClaudeExecutor` spawns real `claude -p` processes. `Scheduler.fill_slots()` takes `&ClaudeExecutor` directly (no trait). This means we cannot test the full Scheduler → Executor path without the CLI.

**The solution**: We don't need to test that `claude -p` returns good output. We need to test that **given a TaskResult**, the system transitions state correctly. The pipeline breaks into two testable halves:

1. **Result → State transitions** (handle_completion, apply_reactor, confirmation gate): All pure functions or small methods that take data and return data. Fully testable.
2. **State → Task scheduling** (fill_slots, confirmation gate in scheduler): Requires Scheduler + ClaudeExecutor. We use `inject_mock_task()` for the execution side, and extract the gate logic into a testable path.

---

## Phase 0: Production Code Changes for Testability

These are minimal, surgical changes. No refactoring, no trait extraction.

### 0.1 — Add `#[cfg(test)]` accessors to Driver

**File**: `redtrail/src/agent/driver.rs`

Add a `#[cfg(test)]` impl block at the bottom:

```rust
#[cfg(test)]
impl Driver {
    pub fn knowledge(&self) -> &KnowledgeBase { &self.knowledge }
    pub fn knowledge_mut(&mut self) -> &mut KnowledgeBase { &mut self.knowledge }
    pub fn tree(&self) -> &TaskTree { &self.tree }
    pub fn tree_mut(&mut self) -> &mut TaskTree { &mut self.tree }
    pub fn all_findings(&self) -> &[Finding] { &self.all_findings }
    pub fn session_complete(&self) -> bool { self.session_complete }

    /// Expose handle_completion for integration tests.
    pub fn test_handle_completion(&mut self, task_id: u32, result: Result<TaskResult, RedtrailError>) {
        self.handle_completion(task_id, result);
    }

    /// Expose apply_reactor for integration tests.
    pub fn test_apply_reactor(&mut self, task_id: u32, task_result: &TaskResult) {
        self.apply_reactor(task_id, task_result);
    }

    /// Create a Driver for testing (no real executor, channels go to /dev/null).
    pub fn test_new(target: Target, session_goal: Option<SessionGoal>) -> Self {
        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (_cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let provider: Arc<dyn LlmProvider> = Arc::new(crate::agent::llm::MockProvider::new());

        Driver::new(
            target,
            provider,
            false,
            2,
            60,
            60,
            session_goal,
            event_tx,
            cmd_rx,
        )
    }
}
```

### 0.2 — Make `parse_strategy_response` public (for testing)

**File**: `redtrail/src/agent/strategist.rs`

Change `fn parse_strategy_response` to `pub fn parse_strategy_response`.

### 0.3 — Make `extract_probe_result` accessible

**File**: `redtrail/src/agent/driver.rs`

The `extract_probe_result` function (used in `apply_reactor`) needs to be accessible. Either move it to a `pub(crate)` module function or ensure it's already public. Check if it's a free function or method.

### 0.4 — Add `Scheduler::fill_slots_dry_run` (optional)

If we want to test the confirmation gate through real Scheduler code without spawning tasks:

```rust
#[cfg(test)]
impl Scheduler {
    /// Like fill_slots but only applies the gate logic and returns
    /// which tasks would be skipped/spawned, without actually spawning.
    pub fn dry_run_gate(
        &self,
        tree: &mut TaskTree,
        kb: &KnowledgeBase,
    ) -> Vec<(u32, bool)> {  // (task_id, would_be_allowed)
        let candidates = tree.next_ready_batch(10);
        let mut results = Vec::new();
        for task_id in candidates {
            let node = match tree.nodes.iter().find(|n| n.id == task_id) {
                Some(n) => n,
                None => continue,
            };
            if let TaskType::ExploitHypothesis { hypothesis_id, .. } = &node.task {
                let is_confirmed = kb
                    .system_model
                    .get_hypothesis(hypothesis_id)
                    .is_some_and(|h| h.status == HypothesisStatus::Confirmed);
                results.push((task_id, is_confirmed));
            } else {
                results.push((task_id, true));
            }
        }
        results
    }
}
```

---

## Phase 1: Driver Integration Tests

**File**: `redtrail/tests/test_driver_integration.rs`

These tests create a real `Driver` (with test constructor), inject `TaskResult`s into `handle_completion`, and verify state transitions through the accessors.

### Test 1.1 — handle_completion merges hosts into KB

```
Setup: Driver with empty KB
Action: handle_completion(task_id, Ok(TaskResult with 2 discovered hosts))
Assert:
  - driver.knowledge().discovered_hosts.len() == 2
  - task status == Completed in tree
```

### Test 1.2 — handle_completion merges credentials with dedup

```
Setup: Driver with KB already containing cred (admin/pass on ssh/10.0.0.1)
Action: handle_completion with result containing same cred + new cred
Assert:
  - driver.knowledge().credentials.len() == 2 (not 3)
```

### Test 1.3 — handle_completion merges flags and updates metrics

```
Setup: Driver with empty KB
Action: handle_completion with result containing 2 flags
Assert:
  - driver.knowledge().flags == ["FLAG{a}", "FLAG{b}"]
  - driver.knowledge().deductive_metrics.flags_captured == 2
```

### Test 1.4 — handle_completion increments deductive metrics per task type

```
Setup: Driver with tree containing DifferentialProbe, BruteForce, and PortScan tasks
Action: Complete each task
Assert:
  - probe_calls == 1
  - brute_force_calls == 1
  - enumeration_calls == 1
  - total_tool_calls == 3
```

### Test 1.5 — handle_completion checks goal criteria and sets session_complete

```
Setup: Driver with CaptureFlags goal (expected: 2)
Action: handle_completion with result containing 2 flags
Assert:
  - driver.knowledge().goal.status == GoalStatus::Achieved
  - driver.session_complete() == true
```

### Test 1.6 — handle_completion records failed task correctly

```
Setup: Driver with tree containing 1 task
Action: handle_completion(task_id, Err(RedtrailError::Parse("timeout")))
Assert:
  - tree node status == Failed("timeout")
  - KB unchanged (no merge happened)
```

### Test 1.7 — apply_reactor updates hypothesis probes on DifferentialProbe

```
Setup: Driver with KB containing hypothesis "h-1" (Proposed, 0 probes)
       Tree with DifferentialProbe task for "h-1"
Action: apply_reactor(task_id, TaskResult with raw output indicating anomaly)
Assert:
  - hypothesis "h-1" now has 1 probe
  - hypothesis "h-1" status == Probing (transitioned from Proposed)
```

### Test 1.8 — apply_reactor emits HypothesisConfirmed and injects ExploitHypothesis

```
Setup: Driver with KB containing hypothesis "h-1" (Probing, probes: [no-anomaly, anomaly])
       Tree with DifferentialProbe task for "h-1"
Action: apply_reactor on that task
Assert:
  - deductive_metrics.hypotheses_confirmed == 1
  - tree now contains ExploitHypothesis task for "h-1"
  - ExploitHypothesis task priority == High
```

### Test 1.9 — apply_reactor emits HypothesisRefuted and cancels tasks

```
Setup: Driver with KB containing hypothesis "h-1" (Probing, 3 clean probes)
       Tree with DifferentialProbe for "h-1" + another pending ExploitHypothesis for "h-1"
Action: apply_reactor on the probe task
Assert:
  - deductive_metrics.hypotheses_refuted == 1
  - ExploitHypothesis task status == Skipped("hypothesis refuted")
```

### Test 1.10 — apply_reactor injects credential spray tasks on CredentialFound

```
Setup: Driver with KB containing:
  - 1 credential (admin/pass on ssh/10.0.0.1)
  - 2 discovered hosts with ssh service
Action: handle_completion with result containing a new credential
Assert:
  - Tree has new TryCredentials tasks for the other hosts
```

### Test 1.11 — apply_reactor injects post-exploit pipeline on AccessGained

```
Setup: Driver with KB containing access_levels and credentials for a host
Action: handle_completion with result containing a new access_level
Assert:
  - Tree has SituationalAwareness, CredentialHarvest, InternalRecon tasks
```

### Test 1.12 — Full pipeline: probe → confirm → exploit injected → goal check

```
Setup: Driver with:
  - CaptureFlags goal (1 flag)
  - Hypothesis "h-1" (Probing, 1 clean probe, 1 anomaly probe)
  - DifferentialProbe task in tree
Action 1: apply_reactor on DifferentialProbe → ExploitHypothesis injected
Action 2: handle_completion on ExploitHypothesis with result containing FLAG{pwned}
Assert:
  - hypothesis confirmed
  - flag captured
  - goal achieved
  - session_complete == true
```

---

## Phase 2: Scheduler Confirmation Gate Tests (Real Code)

**File**: `redtrail/tests/test_scheduler_gate.rs`

These tests use the real `Scheduler` and call either `dry_run_gate` (Phase 0.4) or `fill_slots` with `inject_mock_task` to verify the gate works through production code paths.

### Test 2.1 — fill_slots skips ExploitHypothesis when hypothesis is Proposed

```
Setup: Scheduler + TaskTree with ExploitHypothesis task + KB with hypothesis status=Proposed
Action: scheduler.fill_slots(&mut tree, &executor, &kb)
Assert:
  - Task status == Skipped("confirmation gate: hypothesis not confirmed")
  - scheduler.running_count() == 0
```

### Test 2.2 — fill_slots allows ExploitHypothesis when hypothesis is Confirmed

```
Setup: Same as 2.1 but hypothesis status=Confirmed
Action: scheduler.fill_slots(&mut tree, &executor, &kb)
Assert:
  - Task status == Running
  - scheduler.running_count() == 1
```

### Test 2.3 — fill_slots skips ExploitHypothesis for Refuted hypothesis

Same pattern as 2.1 with Refuted status.

### Test 2.4 — fill_slots skips ExploitHypothesis for missing hypothesis

KB has no hypothesis matching the ID in the task.

### Test 2.5 — Non-exploit tasks bypass the gate entirely

```
Setup: TaskTree with PortScan + WebEnum + ExploitHypothesis(Proposed)
Action: fill_slots
Assert:
  - PortScan and WebEnum are Running
  - ExploitHypothesis is Skipped
```

### Test 2.6 — Gate state change: hypothesis transitions Proposed→Confirmed between fill calls

```
Setup: First fill_slots: ExploitHypothesis is Skipped
       Then: update hypothesis to Confirmed, add new ExploitHypothesis task
       Second fill_slots
Assert:
  - New ExploitHypothesis is Running
```

### Test 2.7 — Service target conflict: two tasks on same host:port

```
Setup: Two PortScan tasks for 10.0.0.1:80
Action: fill_slots
Assert:
  - Only one is Running, the other stays Pending
```

### Test 2.8 — Adaptive scaling: initial=2, max=4

```
Setup: Scheduler(initial=2, max=4), tree with 4 depth-0 tasks (completed) + 4 new tasks
Action: maybe_scale_up, then fill_slots
Assert:
  - current_concurrency == 4
  - 4 tasks running
```

---

## Phase 3: Strategist Parse & Plan Application Tests

**File**: `redtrail/tests/test_strategist_integration.rs`

These test the strategist output parsing and plan application through real code. No LLM calls — we construct raw strategist output strings and parse them.

### Test 3.1 — parse_strategy_response parses valid JSON between markers

```
Input: "reasoning\n===REDTRAIL_STRATEGY===\n{\"assessment\":\"...\",\"tasks\":[],\"is_complete\":false,...}\n===REDTRAIL_STRATEGY===\nmore text"
Assert: Parses into StrategistPlan with correct fields
```

### Test 3.2 — parse_strategy_response rejects output without markers

```
Input: "just some text without markers"
Assert: Err
```

### Test 3.3 — parse_strategy_response rejects malformed JSON between markers

```
Input: "===REDTRAIL_STRATEGY===\n{invalid json}\n===REDTRAIL_STRATEGY==="
Assert: Err with parse error
```

### Test 3.4 — parse_strategy_response handles plan with hypotheses

```
Input: Valid JSON with hypotheses field containing 2 hypotheses
Assert: plan.hypotheses.len() == 2, each has correct category/status
```

### Test 3.5 — parse_strategy_response handles model_updates

```
Input: Valid JSON with model_updates containing AddComponent, AddBoundary
Assert: plan.model_updates parsed correctly
```

### Test 3.6 — parse_strategy_response handles advance_layer

```
Input: Valid JSON with advance_layer: "Hypothesizing"
Assert: plan.advance_layer == Some(DeductiveLayer::Hypothesizing)
```

### Test 3.7 — Deterministic completion override

```
Setup: KB with goal status = InProgress
Input: Strategist returns is_complete: true
Assert: After the override (line 224 in strategist.rs), plan.is_complete == false
  (because goal is not achieved — LLM doesn't decide completion)
```

### Test 3.8 — Deterministic completion when goal achieved

```
Setup: KB with goal status = Achieved
Input: Strategist returns is_complete: false
Assert: plan.is_complete == true (overridden by goal status)
```

### Test 3.9 — Plan application: new definitions registered

```
Setup: Driver with empty registry
Action: Simulate strategist plan with 2 new task definitions
Apply: Driver processes plan (register definitions, enqueue tasks)
Assert:
  - Registry contains the 2 new definitions
  - KB.custom_definitions_used has the definition names
```

### Test 3.10 — Plan application: model updates applied

```
Setup: Driver with empty system model
Action: Simulate strategist plan with AddComponent + AddBoundary
Apply: Driver processes plan
Assert:
  - KB.system_model.components has the new component
  - KB.system_model.trust_boundaries has the new boundary
```

### Test 3.11 — Plan application: cancel filters work

```
Setup: Driver with tree containing 3 pending tasks (2 BruteForce, 1 WebEnum)
Action: Simulate strategist plan with cancel filter for "BruteForce"
Assert:
  - 2 BruteForce tasks are Skipped("strategist cancelled")
  - WebEnum remains Pending
```

### Test 3.12 — Plan application: hypotheses from strategist added to KB

```
Setup: Driver with empty system model
Action: Simulate strategist plan with 3 hypotheses (1 Input, 1 Boundary, 1 State)
Assert:
  - KB.system_model.hypotheses.len() == 3
  - Each has correct category
```

---

## Phase 4: End-to-End Flow Tests (Multi-Step Scenarios)

**File**: `redtrail/tests/test_e2e_deductive_flow.rs`

These simulate complete engagement scenarios through Driver, verifying the full chain from recon through hypothesis testing to exploitation.

### Test 4.1 — Recon → Model → Hypothesize → Probe → Confirm → Exploit → Flag

The grand integration test. Simulates a complete CTF solve:

```
Step 1: Seed Driver with PingSweep result → KB gets discovered hosts
Step 2: Inject PortScan result → KB gets services (http on port 80)
Step 3: Simulate strategist adding a web component + SQLi hypothesis
Step 4: Inject DifferentialProbe result with anomaly → hypothesis confirmed
Step 5: Verify ExploitHypothesis task was injected by reactor
Step 6: Inject ExploitHypothesis result with FLAG{sql_master}
Step 7: Verify:
  - Flag in KB
  - Goal achieved
  - Session complete
  - Deductive metrics: probe_calls >= 1, hypotheses_confirmed == 1
  - Evidence chain has entries
```

### Test 4.2 — Recon → Hypothesize → Probe (clean) → Refute → No Exploit

```
Step 1-3: Same as 4.1 (recon + hypothesis)
Step 4: Inject 3 DifferentialProbe results with NO anomaly
Step 5: Verify hypothesis refuted
Step 6: Verify ExploitHypothesis task was NOT injected (or was cancelled)
Step 7: Verify deductive_metrics.hypotheses_refuted == 1
```

### Test 4.3 — Multiple hypotheses: one confirmed, one refuted

```
Setup: 2 hypotheses (h-sqli, h-xss)
Step 1: Probe h-sqli with anomaly → confirmed
Step 2: Probe h-xss with 3 clean → refuted
Step 3: Verify:
  - ExploitHypothesis for h-sqli exists and is allowed by gate
  - ExploitHypothesis for h-xss was cancelled
```

### Test 4.4 — Credential found → spray → access → post-exploit pipeline

```
Step 1: Task result with credential (admin/pass123 on ssh/10.0.0.1)
Step 2: KB has 2 other hosts with ssh service
Step 3: Verify spray tasks injected (TryCredentials for each host)
Step 4: Simulate TryCredentials success with AccessGained
Step 5: Verify post-exploit pipeline (SituationalAwareness, CredentialHarvest, InternalRecon)
```

### Test 4.5 — Defender model integration: noise budget affects scheduling

```
Step 1: Record multiple blocks → noise_budget drops
Step 2: Verify noisy tasks get deprioritized (adjusted_priority < base)
Step 3: Verify silent tasks still allowed at low budget
```

### Test 4.6 — Goal system integration: partial achievement

```
Setup: Multi-criteria goal (2 flags + 1 high vuln)
Step 1: Capture 2 flags → first criterion met
Step 2: Verify goal.status == PartiallyAchieved
Step 3: Find high vuln → second criterion met
Step 4: Verify goal.status == Achieved, session_complete == true
```

### Test 4.7 — Cross-session memory integration

```
Step 1: Create in-memory RedtrailDb
Step 2: Record attack patterns and technique executions
Step 3: Build relevance query from KB
Step 4: Gather cross-session intel
Step 5: Verify intel contains relevant patterns
```

### Test 4.8 — Error recovery: failed task doesn't corrupt state

```
Step 1: Complete task A successfully (KB has data)
Step 2: Task B fails with error
Step 3: Verify:
  - Task B marked as Failed
  - KB still has Task A's data (not rolled back)
  - Session not marked complete
  - Metrics still correct
```

---

## Phase 5: ClaudeExecutor Unit Tests (No Process Spawn)

**File**: `redtrail/tests/test_executor_parsing.rs`

These test the parsing and merging logic in ClaudeExecutor without spawning `claude -p`.

### Test 5.1 — parse_result_markers: valid JSON between markers

```
Input: "text ===REDTRAIL_RESULT=== {\"discovered_hosts\":[...]} ===REDTRAIL_RESULT=== more"
Assert: ParseOutcome::Success with correct data
```

### Test 5.2 — parse_result_markers: no markers returns NoMarkers

### Test 5.3 — parse_result_markers: invalid JSON returns ParseError

### Test 5.4 — parse_result_markers: only 1 marker returns NoMarkers

### Test 5.5 — merge_result_into_kb: hosts merged with dedup

```
Setup: KB with host 10.0.0.1 (ports [22])
Action: merge with result containing host 10.0.0.1 (ports [22, 80])
Assert: KB has 10.0.0.1 with ports [22, 80]
```

### Test 5.6 — merge_result_into_kb: credentials merged with dedup

### Test 5.7 — merge_result_into_kb: flags merged with dedup

### Test 5.8 — merge_result_into_kb: access levels merged

### Test 5.9 — convert_findings: converts FindingReport to Finding correctly

### Test 5.10 — merge_result_into_kb: OS field updated only when None

```
Setup: KB with host (os: Some("Linux"))
Action: merge with host (os: Some("Ubuntu"))
Assert: OS still "Linux" (doesn't overwrite)
```

---

## Phase 6: TaskTree Behavior Tests

**File**: `redtrail/tests/test_task_tree_behavior.rs`

### Test 6.1 — next_ready_batch returns highest priority first

### Test 6.2 — cancel_by_hypothesis cancels all tasks for that hypothesis

### Test 6.3 — is_queue_empty_and_idle when all tasks completed

### Test 6.4 — has_task_for correctly detects existing tasks

### Test 6.5 — build_task_prompt includes KB context

### Test 6.6 — task_allowed_tools returns correct tools per task type

### Test 6.7 — Background tasks: next_ready_background_batch

---

## Implementation Order

1. **Phase 0** — Code changes (1-2 hours). Small, safe, `#[cfg(test)]` only.
2. **Phase 5** — Executor parsing tests (easiest, pure functions, no async).
3. **Phase 1** — Driver integration tests (core value — proves the wiring).
4. **Phase 2** — Scheduler gate tests (proves the safety invariant).
5. **Phase 3** — Strategist tests (proves plan parsing and application).
6. **Phase 4** — E2E flow tests (the crown jewels — proves Redtrail works).
7. **Phase 6** — TaskTree behavior tests (completeness).

## What This Proves

After all phases are complete:

- **The confirmation gate works through real Scheduler code** (Phase 2)
- **handle_completion correctly wires merge → reactor → goal check** (Phase 1)
- **apply_reactor correctly transitions hypothesis status, classifies events, and injects/cancels tasks** (Phase 1)
- **A full engagement from recon to flag capture flows through the correct code paths** (Phase 4)
- **Negative cases: clean probes don't exploit, failed tasks don't corrupt state** (Phase 4)
- **Strategist output is parsed and applied correctly** (Phase 3)
- **Deterministic completion: goals decide when to stop, not the LLM** (Phase 3)
- **Cross-session memory round-trips correctly** (Phase 4)

## What This Does NOT Prove

- That the LLM (Claude CLI) produces good specialist output
- That the strategist prompt engineering yields good plans
- That tool execution against real targets works

These require live testing against actual CTF labs, which is a separate validation track.
