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

## Testing Strategy (TDD — Red-Green-Refactor)

Tests are written as vertical slices: one test → one implementation → repeat.
Each test verifies behavior through public interfaces (`detect_phase()`,
`load_skill_prompt()`, `build_system_prompt()`), not implementation details.
Tests should survive internal refactors.

### Tracer Bullet

Start with the simplest end-to-end path:

1. **RED**: `detect_phase()` with empty KB returns `SkillMatch { skill_name: "redtrail-recon" }`
2. **GREEN**: implement the first rule only

This proves the DB query → rule evaluation → SkillMatch path works.

### Incremental Behaviors (one cycle each, in order)

Each behavior is a RED→GREEN cycle. Order follows dependency chain.

**Phase detection behaviors** (test through `detect_phase()` public API):

1. Empty KB (hosts=0, hypotheses=0) → redtrail-recon
2. Hosts exist, no hypotheses → redtrail-hypothesize
3. Pending hypotheses exist → redtrail-probe
4. Confirmed hypotheses, none pending → redtrail-exploit
5. All refuted (pending=0, confirmed=0, refuted>0) → redtrail-recon (widen)
6. No rules match → returns None

**Skill loading behaviors** (test through `load_skill_prompt()` public API):

7. Loads prompt.md from workspace skills directory
8. Loads from ~/.redtrail/skills/ when installed (takes precedence over workspace)
9. Returns `Error::SkillNotFound` with clear message for nonexistent skill

**System prompt integration behaviors** (test through `build_system_prompt()`):

10. Auto-detected skill replaces generic identity in system prompt
11. `--skill` override loads specified skill regardless of phase
12. `--no-skill` produces generic identity prompt
13. No skill match falls back to generic identity
14. Skill prompt is followed by KB dump (hosts, hypotheses, etc.)

### Refactor

After all behaviors pass, look for:
- Duplication between detection rules
- Whether `build_system_prompt()` can be simplified
- Whether skill resolution logic should be extracted

### What NOT to test

- Internal helper functions (test through public API)
- Exact prompt string formatting (test that skill content is present, not exact layout)
- API call behavior (unchanged, already covered by existing tests)
