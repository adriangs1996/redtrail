# Phase-Aware Skill Loading for `ask` and `query`

## Problem

The `ask` and `query` commands build a generic system prompt with KB context but
have zero awareness of skills. Skills exist as files (`skill.toml` + `prompt.md`)
but are never loaded into the LLM conversation. The operator gets generic advice
instead of phase-specific methodology guidance.

## Decision Summary

| Decision | Choice |
|----------|--------|
| Auto-detect vs explicit | Hybrid: auto-detect phase + `--skill` override |
| Phase detection location | Rust-side (deterministic, zero latency) |
| Orchestrator prompt.md | Untouched — stays authoritative for external agents |
| Skill scope per request | Single matched skill only (no orchestrator prepended) |
| ask vs query behavior | Both load skills identically, every call (query remains stateless) |
| Skill placement in prompt | Replaces generic identity block when active |

## New Module: `src/skill_loader.rs`

### Phase Detection

```rust
pub struct SkillMatch {
    pub phase_name: String,   // "Setup", "Hypotheses Pending", etc.
    pub skill_name: String,   // "redtrail-recon", "redtrail-probe", etc.
    pub context: String,      // e.g. "3 pending hypotheses"
}

pub fn detect_phase(conn: &Connection, session_id: &str) -> Result<Option<SkillMatch>, Error>
```

Rules applied in order (first match wins):

| # | Condition | Phase Name | Skill | Context |
|---|-----------|------------|-------|---------|
| 1 | hosts=0 AND hypotheses=0 | Setup | redtrail-recon | "no hosts discovered" |
| 2 | hosts>0 AND hypotheses=0 | Surface Mapped | redtrail-hypothesize | "{n} hosts, no hypotheses" |
| 3 | any hypothesis pending | Hypotheses Pending | redtrail-probe | "{n} pending" |
| 4 | any confirmed, none pending | Confirmed Available | redtrail-exploit | "{n} confirmed" |
| 5 | pending=0 AND confirmed=0 AND refuted>0 | Surface Exhausted | redtrail-recon | "all {n} refuted, widening" |

Deferred rules (require schema changes):
- **Objective Met** (goal achieved → redtrail-report): No `goal_status` column
  exists in the sessions table. Requires adding a `goal_status TEXT DEFAULT 'active'`
  column or inferring from flags count. Deferred to a follow-up migration.
- **New Credentials** (credential count increased → redtrail-hypothesize): Requires
  tracking previous credential count across invocations.

If no rule matches (e.g., no session data at all), returns `None` and the
generic identity is used as fallback.

### Skill Loading

```rust
pub fn load_skill_prompt(skill_name: &str) -> Result<String, Error>
```

Resolution order:
1. `~/.redtrail/skills/<name>/prompt.md` — installed skills (user overrides)
2. `<workspace>/skills/<name>/prompt.md` — bundled skills (fallback)

Returns the raw markdown content of `prompt.md`. Returns
`Error::SkillNotFound(name)` with message "skill '{name}' not found in
~/.redtrail/skills/ or workspace skills/" if not found in any location.

## Changes to `src/cli/ask.rs`

### New CLI Flag

Both `Ask` and `Query` commands gain:

```
--skill <name>      Override auto-detected skill (e.g. --skill redtrail-recon)
--no-skill          Suppress skill auto-detection, use generic advisor prompt
```

When `--skill` is provided, it bypasses `detect_phase()` entirely and loads the
named skill directly. No phase guard — the operator knows better.

When `--no-skill` is provided, no skill is loaded regardless of phase state.

### Modified `build_system_prompt()`

Current signature:
```rust
fn build_system_prompt(conn, session_id, cwd) -> Result<String, Error>
```

New signature:
```rust
fn build_system_prompt(conn, session_id, cwd, skill_override: Option<&str>) -> Result<String, Error>
```

Logic:

1. Resolve skill: if `skill_override` is `Some`, load that skill. Otherwise call
   `detect_phase()` and load the matched skill. If neither produces a skill,
   use the current generic identity block.

2. Build prompt:

```
// When skill is active:
Active skill: {skill_name} ({phase_name} — {context})
---
{skill prompt.md content}
---
Target: {target}
Scope: {scope}
Goal: {goal}
Noise budget: {noise}
CWD: {cwd}

=== Hosts ===
  ...
=== Ports ===
  ...
=== Credentials ===
  ...
=== Flags ===
  ...
=== Access ===
  ...
=== Hypotheses ===
  ...
=== Notes ===
  ...
=== Recent Commands ===
  ...
You have two tools:
- run_command: execute shell commands
- sql_query: query the redtrail database (read-only)
```

```
// When no skill (fallback):
You are Redtrail, a pentesting advisor embedded in a workspace...
{same KB dump and tools as above}
```

### Phase Announcement

When a skill is auto-detected, print a one-line announcement to stderr before
the API call:

```
[phase] Hypotheses Pending (3 pending) — loading redtrail-probe
```

When `--skill` override is used:

```
[skill] loading redtrail-recon (manual override)
```

## What Stays the Same

- Tool definitions (`run_command`, `sql_query`) — unchanged
- API call logic (`call_api`) — unchanged
- Chat history behavior — unchanged
- `query` vs `ask` semantics — unchanged
- Orchestrator `skills/redtrail/prompt.md` — untouched
- `rt skill` subcommands (init, test, list, install, remove) — unchanged

## File Changes Summary

| File | Change |
|------|--------|
| `src/skill_loader.rs` | **New** — `detect_phase()`, `load_skill_prompt()` |
| `src/cli/ask.rs` | Modified — `--skill` flag, `build_system_prompt()` uses skill loader |
| `src/cli/mod.rs` | Modified — add `--skill` arg to Ask and Query variants |
| `src/main.rs` or `src/lib.rs` | Modified — `mod skill_loader;` |

## Testing Strategy

### Unit Tests (`src/skill_loader.rs`)

- `test_detect_phase_setup` — empty KB returns redtrail-recon
- `test_detect_phase_surface_mapped` — hosts exist, no hypotheses → redtrail-hypothesize
- `test_detect_phase_pending` — pending hypotheses → redtrail-probe
- `test_detect_phase_confirmed` — confirmed, no pending → redtrail-exploit
- `test_detect_phase_all_refuted` — pending=0, confirmed=0, refuted>0 → redtrail-recon (widen)
- `test_detect_phase_no_match` — returns None when no rules match
- `test_load_skill_prompt_bundled` — loads from workspace/skills/
- `test_load_skill_prompt_not_found` — returns error for nonexistent skill

### Integration Tests

- `test_ask_auto_loads_skill` — verify system prompt contains skill content
- `test_ask_skill_override` — `--skill` flag loads specified skill
- `test_ask_no_skill` — `--no-skill` flag suppresses auto-detection
- `test_query_loads_skill` — query command gets skill in system prompt
- `test_fallback_generic_identity` — no skill match → generic prompt
- `test_skill_not_found_error` — `--skill nonexistent` returns clear error message
