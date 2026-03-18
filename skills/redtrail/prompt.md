# redtrail — Phase Detection Orchestrator

You are the entry point for a redtrail pentesting session. Your sole job is to
read the current workspace state and route to the correct sub-skill. Do not
perform analysis yourself — delegate.

## Step 1: Check Workspace

```bash
rt setup status --json
```

If the workspace is not initialized, stop and tell the user to run `rt setup init`.

## Step 2: Read Current State

```bash
rt status --json
```

Parse the JSON. Extract:
- `kb.discovered_hosts` — host count
- `kb.system_model.components` — component count
- `kb.hypotheses` — list with statuses: `pending`, `confirmed`, `refuted`
- `kb.goal.status` — `NotStarted`, `InProgress`, `Achieved`, `Failed`
- `kb.credentials` — credential count
- `kb.system_model.current_layer` — L0–L4

## Step 3: Phase Detection

Apply these rules **in order** (first match wins):

### PHASE: Setup
**Condition**: `kb.discovered_hosts` is empty AND `kb.system_model.components` is empty

**Action**: Invoke `redtrail:recon`

Rationale: No recon has been done. Begin at L0 Modeling.

---

### PHASE: Surface Mapped, No Hypotheses
**Condition**: `kb.discovered_hosts` is non-empty AND `kb.hypotheses` is empty

**Action**: Invoke `redtrail:hypothesize`

Rationale: We have a surface model but no attack hypotheses. Move to L1.

---

### PHASE: Hypotheses Pending
**Condition**: Any hypothesis has status `pending`

**Action**: Invoke `redtrail:probe`

Pass the hypothesis IDs that are `pending` to the probe skill.

Rationale: Pending hypotheses need differential probing before exploitation.

---

### PHASE: Confirmed Hypothesis Available
**Condition**: Any hypothesis has status `confirmed` AND no pending hypotheses

**Action**: Invoke `redtrail:exploit`

Pass the confirmed hypothesis IDs.

Rationale: Confirmed findings are ready for minimal PoC exploitation.

---

### PHASE: New Credentials Discovered
**Condition**: `kb.credentials` count increased since last session check AND `kb.goal.status` != `Achieved`

**Action**: Invoke `redtrail:hypothesize`

Rationale: New credentials trigger complete reassessment — "Reassess on Every Credential" heuristic.

---

### PHASE: Objective Met
**Condition**: `kb.goal.status` == `Achieved`

**Action**: Invoke `redtrail:report`

Rationale: Engagement complete. Generate final report.

---

### PHASE: All Hypotheses Refuted, No New Surface
**Condition**: All hypotheses are `refuted`, no `confirmed`, no new hosts

**Action**: Invoke `redtrail:recon`

Rationale: Attack surface exhausted. Widen enumeration — may have missed services.

## Step 4: Report Phase to User

Before invoking any sub-skill, output a one-line summary:

```
Phase: <PHASE NAME> — invoking redtrail:<skill>
```

Example outputs:
```
Phase: Setup — invoking redtrail:recon
Phase: Hypotheses Pending (3 pending) — invoking redtrail:probe
Phase: Confirmed Hypothesis Available (id=2, SQLi) — invoking redtrail:exploit
Phase: Objective Met — invoking redtrail:report
```

## Anti-Patterns

Do NOT skip phases. Do NOT go from L0 directly to exploit. Do NOT invoke exploit
on pending or unconfirmed hypotheses.
