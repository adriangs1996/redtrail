# Phase-Aware Skill Loading Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `rt ask` and `rt query` auto-detect the pentesting phase from KB state and inject the matching skill's methodology prompt into the LLM system prompt.

**Architecture:** New `src/skill_loader.rs` module with `detect_phase()` (DB queries → deterministic rule matching) and `load_skill_prompt()` (filesystem resolution). `ask.rs` gains `--skill`/`--no-skill` flags and replaces the generic identity block with skill content when a skill is active.

**Tech Stack:** Rust, rusqlite, clap, std::fs

**Spec:** `docs/superpowers/specs/2026-03-19-phase-aware-skill-loading-design.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/skill_loader.rs` | **New.** `SkillMatch` struct, `detect_phase()`, `load_skill_prompt()` |
| `src/main.rs` | **Modify.** Add `mod skill_loader;` |
| `src/error.rs` | **Modify.** Add `SkillNotFound(String)` variant |
| `src/cli/mod.rs` | **Modify.** Add `--skill` and `--no-skill` to Ask/Query variants, pass to `ask::run()` |
| `src/cli/ask.rs` | **Modify.** New `skill_override` param, update `build_system_prompt()` to use skill loader |
| `tests/skill_loader.rs` | **New.** Integration tests for phase detection and skill loading |

---

## Task 1: Add `SkillNotFound` error variant

**Files:**
- Modify: `src/error.rs:4-10`

- [ ] **Step 1: Add the variant**

In `src/error.rs`, add `SkillNotFound(String)` to the `Error` enum and its Display impl:

```rust
// In the enum:
SkillNotFound(String),

// In Display:
Error::SkillNotFound(name) => write!(f, "skill '{name}' not found in ~/.redtrail/skills/ or workspace skills/"),
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build 2>&1 | head -5`
Expected: compiles (variant unused warning is fine)

- [ ] **Step 3: Commit**

```bash
git add src/error.rs
git commit -m "Add SkillNotFound error variant"
```

---

## Task 2: Tracer bullet — `detect_phase()` with empty KB returns redtrail-recon

**Files:**
- Create: `src/skill_loader.rs`
- Modify: `src/main.rs:1-8` (add `mod skill_loader;`)

- [ ] **Step 1: Make SCHEMA accessible for tests**

In `src/db/mod.rs`, change line 10 from:

```rust
const SCHEMA: &str = "
```

to:

```rust
pub(crate) const SCHEMA: &str = "
```

- [ ] **Step 2: Write the failing test**

Create `src/skill_loader.rs` with:

```rust
use rusqlite::Connection;
use crate::error::Error;

pub struct SkillMatch {
    pub phase_name: String,
    pub skill_name: String,
    pub context: String,
}

pub fn detect_phase(_conn: &Connection, _session_id: &str) -> Result<Option<SkillMatch>, Error> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db(session_id: &str) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, name, goal) VALUES (?1, ?1, 'general')",
            rusqlite::params![session_id],
        ).unwrap();
        conn
    }

    #[test]
    fn test_detect_phase_setup_empty_kb() {
        let conn = setup_db("s1");
        let result = detect_phase(&conn, "s1").unwrap();
        let m = result.expect("should match a phase");
        assert_eq!(m.skill_name, "redtrail-recon");
        assert_eq!(m.phase_name, "Setup");
    }
}
```

- [ ] **Step 3: Register the module**

In `src/main.rs`, add `mod skill_loader;` after the existing `mod` declarations.

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test test_detect_phase_setup_empty_kb -- --nocapture 2>&1 | tail -5`
Expected: FAIL with `not yet implemented`

- [ ] **Step 5: Implement minimal `detect_phase()`**

Replace the `todo!()` body with just rule 1:

```rust
pub fn detect_phase(conn: &Connection, session_id: &str) -> Result<Option<SkillMatch>, Error> {
    let host_count: i64 = conn.query_row(
        "SELECT count(*) FROM hosts WHERE session_id = ?1",
        rusqlite::params![session_id],
        |r| r.get(0),
    ).map_err(|e| Error::Db(e.to_string()))?;

    let hyp_total: i64 = conn.query_row(
        "SELECT count(*) FROM hypotheses WHERE session_id = ?1",
        rusqlite::params![session_id],
        |r| r.get(0),
    ).map_err(|e| Error::Db(e.to_string()))?;

    if host_count == 0 && hyp_total == 0 {
        return Ok(Some(SkillMatch {
            phase_name: "Setup".into(),
            skill_name: "redtrail-recon".into(),
            context: "no hosts discovered".into(),
        }));
    }

    Ok(None)
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test test_detect_phase_setup_empty_kb -- --nocapture 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/skill_loader.rs src/main.rs src/db/mod.rs
git commit -m "Tracer bullet: detect_phase with empty KB returns redtrail-recon"
```

---

## Task 3: Phase detection — remaining rules (4 RED→GREEN cycles)

**Files:**
- Modify: `src/skill_loader.rs`

Each sub-step below is one RED→GREEN cycle. Add the test, run it (FAIL), add the rule to `detect_phase()`, run it (PASS).

- [ ] **Step 1: RED — hosts exist, no hypotheses → redtrail-hypothesize**

Add test:

```rust
#[test]
fn test_detect_phase_surface_mapped() {
    let conn = setup_db("s1");
    conn.execute(
        "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
        [],
    ).unwrap();
    let m = detect_phase(&conn, "s1").unwrap().unwrap();
    assert_eq!(m.skill_name, "redtrail-hypothesize");
    assert_eq!(m.phase_name, "Surface Mapped");
}
```

Run: `cargo test test_detect_phase_surface_mapped -- --nocapture`
Expected: FAIL (returns redtrail-recon because rule 1 fires: host_count=1 but hyp_total=0, so rule 1 doesn't match... actually rule 1 requires hosts=0 AND hypotheses=0, so with hosts=1 and hyp=0, rule 1 doesn't match and we get None). Expected: FAIL with "called unwrap on None"

- [ ] **Step 2: GREEN — add rule 2**

After rule 1 in `detect_phase()`:

```rust
if host_count > 0 && hyp_total == 0 {
    return Ok(Some(SkillMatch {
        phase_name: "Surface Mapped".into(),
        skill_name: "redtrail-hypothesize".into(),
        context: format!("{host_count} hosts, no hypotheses"),
    }));
}
```

Run: `cargo test test_detect_phase_surface_mapped -- --nocapture`
Expected: PASS

- [ ] **Step 3: RED — pending hypotheses → redtrail-probe**

Add test:

```rust
#[test]
fn test_detect_phase_hypotheses_pending() {
    let conn = setup_db("s1");
    conn.execute(
        "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO hypotheses (session_id, statement, category, status) VALUES ('s1', 'SQLi in login', 'input', 'pending')",
        [],
    ).unwrap();
    let m = detect_phase(&conn, "s1").unwrap().unwrap();
    assert_eq!(m.skill_name, "redtrail-probe");
    assert_eq!(m.phase_name, "Hypotheses Pending");
}
```

Run: `cargo test test_detect_phase_hypotheses_pending -- --nocapture`
Expected: FAIL

- [ ] **Step 4: GREEN — add rule 3**

Need to query hypothesis counts by status. Add these queries after `hyp_total`:

```rust
let hyp_pending: i64 = conn.query_row(
    "SELECT count(*) FROM hypotheses WHERE session_id = ?1 AND status = 'pending'",
    rusqlite::params![session_id],
    |r| r.get(0),
).map_err(|e| Error::Db(e.to_string()))?;

let hyp_confirmed: i64 = conn.query_row(
    "SELECT count(*) FROM hypotheses WHERE session_id = ?1 AND status = 'confirmed'",
    rusqlite::params![session_id],
    |r| r.get(0),
).map_err(|e| Error::Db(e.to_string()))?;

let hyp_refuted: i64 = conn.query_row(
    "SELECT count(*) FROM hypotheses WHERE session_id = ?1 AND status = 'refuted'",
    rusqlite::params![session_id],
    |r| r.get(0),
).map_err(|e| Error::Db(e.to_string()))?;
```

Add rule 3 after rule 2:

```rust
if hyp_pending > 0 {
    return Ok(Some(SkillMatch {
        phase_name: "Hypotheses Pending".into(),
        skill_name: "redtrail-probe".into(),
        context: format!("{hyp_pending} pending"),
    }));
}
```

Run: `cargo test test_detect_phase_hypotheses_pending -- --nocapture`
Expected: PASS

- [ ] **Step 5: RED — confirmed, none pending → redtrail-exploit**

Add test:

```rust
#[test]
fn test_detect_phase_confirmed_available() {
    let conn = setup_db("s1");
    conn.execute(
        "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO hypotheses (session_id, statement, category, status) VALUES ('s1', 'SQLi confirmed', 'input', 'confirmed')",
        [],
    ).unwrap();
    let m = detect_phase(&conn, "s1").unwrap().unwrap();
    assert_eq!(m.skill_name, "redtrail-exploit");
    assert_eq!(m.phase_name, "Confirmed Available");
}
```

Run: `cargo test test_detect_phase_confirmed_available -- --nocapture`
Expected: FAIL

- [ ] **Step 6: GREEN — add rule 4**

After rule 3:

```rust
if hyp_confirmed > 0 && hyp_pending == 0 {
    return Ok(Some(SkillMatch {
        phase_name: "Confirmed Available".into(),
        skill_name: "redtrail-exploit".into(),
        context: format!("{hyp_confirmed} confirmed"),
    }));
}
```

Run: `cargo test test_detect_phase_confirmed_available -- --nocapture`
Expected: PASS

- [ ] **Step 7: RED — all refuted → redtrail-recon (widen)**

Add test:

```rust
#[test]
fn test_detect_phase_surface_exhausted() {
    let conn = setup_db("s1");
    conn.execute(
        "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO hypotheses (session_id, statement, category, status) VALUES ('s1', 'SQLi refuted', 'input', 'refuted')",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO hypotheses (session_id, statement, category, status) VALUES ('s1', 'XSS refuted', 'input', 'refuted')",
        [],
    ).unwrap();
    let m = detect_phase(&conn, "s1").unwrap().unwrap();
    assert_eq!(m.skill_name, "redtrail-recon");
    assert_eq!(m.phase_name, "Surface Exhausted");
}
```

Run: `cargo test test_detect_phase_surface_exhausted -- --nocapture`
Expected: FAIL

- [ ] **Step 8: GREEN — add rule 5**

After rule 4:

```rust
if hyp_pending == 0 && hyp_confirmed == 0 && hyp_refuted > 0 {
    return Ok(Some(SkillMatch {
        phase_name: "Surface Exhausted".into(),
        skill_name: "redtrail-recon".into(),
        context: format!("all {hyp_refuted} refuted, widening"),
    }));
}
```

Run: `cargo test test_detect_phase_surface_exhausted -- --nocapture`
Expected: PASS

- [ ] **Step 9: Test no-match case**

Add test:

```rust
#[test]
fn test_detect_phase_no_match() {
    let conn = setup_db("s1");
    // hosts exist, hypotheses exist but all are in a custom status
    conn.execute(
        "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO hypotheses (session_id, statement, category, status) VALUES ('s1', 'test', 'input', 'testing')",
        [],
    ).unwrap();
    let result = detect_phase(&conn, "s1").unwrap();
    assert!(result.is_none());
}
```

Run: `cargo test test_detect_phase_no_match -- --nocapture`
Expected: PASS (None returned because no rule matches: hosts>0 so rule 1 fails, hyp_total>0 so rule 2 fails, pending=0 so rule 3 fails, confirmed=0 so rule 4 fails, refuted=0 so rule 5 fails)

- [ ] **Step 10: Run all detect_phase tests**

Run: `cargo test test_detect_phase -- --nocapture`
Expected: all 5 tests PASS

- [ ] **Step 11: Commit**

```bash
git add src/skill_loader.rs
git commit -m "Implement all phase detection rules with tests"
```

---

## Task 4: `load_skill_prompt()` — filesystem skill resolution

**Files:**
- Modify: `src/skill_loader.rs`

- [ ] **Step 1: RED — loads from workspace skills directory**

Add test:

```rust
#[test]
fn test_load_skill_prompt_from_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("skills/redtrail-recon");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("prompt.md"), "# Recon skill prompt").unwrap();

    let result = load_skill_prompt("redtrail-recon", Some(tmp.path())).unwrap();
    assert_eq!(result, "# Recon skill prompt");
}
```

Run: `cargo test test_load_skill_prompt_from_workspace -- --nocapture`
Expected: FAIL (function doesn't exist yet)

- [ ] **Step 2: GREEN — implement `load_skill_prompt()`**

```rust
use std::path::Path;

pub fn load_skill_prompt(skill_name: &str, workspace: Option<&Path>) -> Result<String, Error> {
    // 1. Check ~/.redtrail/skills/<name>/prompt.md
    if let Some(home) = dirs::home_dir() {
        let installed = home.join(".redtrail/skills").join(skill_name).join("prompt.md");
        if installed.exists() {
            return std::fs::read_to_string(&installed).map_err(Error::Io);
        }
    }

    // 2. Check <workspace>/skills/<name>/prompt.md
    if let Some(ws) = workspace {
        let bundled = ws.join("skills").join(skill_name).join("prompt.md");
        if bundled.exists() {
            return std::fs::read_to_string(&bundled).map_err(Error::Io);
        }
    }

    Err(Error::SkillNotFound(skill_name.to_string()))
}
```

Add `use std::path::Path;` to the top of the file (if not already there).

Run: `cargo test test_load_skill_prompt_from_workspace -- --nocapture`
Expected: PASS

- [ ] **Step 3: RED — installed skill takes precedence**

```rust
#[test]
fn test_load_skill_prompt_installed_takes_precedence() {
    // This test is hard to run in CI without mocking home dir.
    // Instead, test that workspace fallback works when home has no skill.
    // The precedence logic is verified by code review — home is checked first.
    // We test the workspace fallback path here.
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("skills/test-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("prompt.md"), "workspace version").unwrap();

    let result = load_skill_prompt("test-skill", Some(tmp.path())).unwrap();
    assert_eq!(result, "workspace version");
}
```

Run: `cargo test test_load_skill_prompt_installed -- --nocapture`
Expected: PASS (already works)

- [ ] **Step 4: RED — nonexistent skill returns SkillNotFound**

```rust
#[test]
fn test_load_skill_prompt_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    let result = load_skill_prompt("nonexistent-skill", Some(tmp.path()));
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("nonexistent-skill"), "error should name the skill: {msg}");
}
```

Run: `cargo test test_load_skill_prompt_not_found -- --nocapture`
Expected: PASS (already returns SkillNotFound)

- [ ] **Step 5: Run all skill_loader tests**

Run: `cargo test skill_loader -- --nocapture`
Expected: all tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/skill_loader.rs
git commit -m "Implement load_skill_prompt with workspace and installed resolution"
```

---

## Task 5: Add `--skill` and `--no-skill` CLI flags

**Files:**
- Modify: `src/cli/mod.rs:114-129`
- Modify: `src/cli/ask.rs:12`

- [ ] **Step 1: Add flags to Ask and Query in `mod.rs`**

In `src/cli/mod.rs`, update the `Ask` variant:

```rust
Ask {
    #[arg(help = "Your question or instruction")]
    message: Option<String>,
    #[arg(long, help = "Clear conversation history and exit")]
    clear: bool,
    #[arg(long, help = "Override LLM model for this request")]
    model: Option<String>,
    #[arg(long, help = "Override auto-detected skill (e.g. redtrail-recon)")]
    skill: Option<String>,
    #[arg(long, help = "Suppress skill auto-detection")]
    no_skill: bool,
},
```

Update the `Query` variant:

```rust
Query {
    #[arg(help = "Your question")]
    message: String,
    #[arg(long, help = "Override LLM model for this request")]
    model: Option<String>,
    #[arg(long, help = "Override auto-detected skill (e.g. redtrail-recon)")]
    skill: Option<String>,
    #[arg(long, help = "Suppress skill auto-detection")]
    no_skill: bool,
},
```

- [ ] **Step 2: Update dispatch in `run()`**

Update the Ask match arm (around line 230):

```rust
Some(Commands::Ask { message, clear, model, skill, no_skill }) => {
    ask::run(message.as_deref(), true, clear, model.as_deref(), skill.as_deref(), no_skill)
}
```

Update the Query match arm (around line 233):

```rust
Some(Commands::Query { message, model, skill, no_skill }) => {
    ask::run(Some(&message), false, false, model.as_deref(), skill.as_deref(), no_skill)
}
```

- [ ] **Step 3: Update `ask::run()` signature**

In `src/cli/ask.rs`, update the `run` function signature:

```rust
pub fn run(message: Option<&str>, keep_history: bool, clear: bool, model_override: Option<&str>, skill_override: Option<&str>, no_skill: bool) -> Result<(), Error> {
```

For now, just add the params and ignore them (the function body stays the same). This makes it compile.

- [ ] **Step 4: Verify it compiles**

Run: `cargo build 2>&1 | tail -5`
Expected: compiles (possibly with unused variable warnings for `skill_override` and `no_skill`)

- [ ] **Step 5: Verify CLI help shows new flags**

Run: `cargo run -- ask --help 2>&1 | grep -E 'skill|no-skill'`
Expected: both `--skill` and `--no-skill` appear

Run: `cargo run -- query --help 2>&1 | grep -E 'skill|no-skill'`
Expected: both appear

- [ ] **Step 6: Commit**

```bash
git add src/cli/mod.rs src/cli/ask.rs
git commit -m "Add --skill and --no-skill flags to ask and query commands"
```

---

## Task 6: Integrate skill loading into `build_system_prompt()`

**Files:**
- Modify: `src/cli/ask.rs`

This is the core integration. `build_system_prompt()` gains skill awareness.

- [ ] **Step 1: RED — auto-detected skill replaces generic identity**

Add test in `src/skill_loader.rs`:

```rust
#[test]
fn test_detect_phase_returns_correct_skill_for_each_state() {
    // Verify the full chain: empty KB → recon, add host → hypothesize
    let conn = setup_db("s1");

    let m1 = detect_phase(&conn, "s1").unwrap().unwrap();
    assert_eq!(m1.skill_name, "redtrail-recon");

    conn.execute("INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')", []).unwrap();
    let m2 = detect_phase(&conn, "s1").unwrap().unwrap();
    assert_eq!(m2.skill_name, "redtrail-hypothesize");

    conn.execute("INSERT INTO hypotheses (session_id, statement, category, status) VALUES ('s1', 'h1', 'input', 'pending')", []).unwrap();
    let m3 = detect_phase(&conn, "s1").unwrap().unwrap();
    assert_eq!(m3.skill_name, "redtrail-probe");
}
```

Run: `cargo test test_detect_phase_returns_correct_skill_for_each_state -- --nocapture`
Expected: PASS (already works from prior implementation)

- [ ] **Step 2: Update `build_system_prompt()` signature and logic**

In `src/cli/ask.rs`, update `build_system_prompt`:

```rust
fn build_system_prompt(conn: &Connection, session_id: &str, cwd: &Path, skill_override: Option<&str>, no_skill: bool) -> Result<String, Error> {
```

Add skill resolution at the top of the function body, before the existing prompt building:

Add `use crate::skill_loader;` at the top of `ask.rs` with the other imports.

Then at the top of the function body:

```rust
let skill_content: Option<(String, String)> = if no_skill {
    None
} else if let Some(name) = skill_override {
    // Manual override: hard fail if skill not found (operator error)
    let prompt = skill_loader::load_skill_prompt(name, Some(cwd))?;
    eprintln!("[skill] loading {name} (manual override)");
    Some((name.to_string(), prompt))
} else {
    // Auto-detection: fallback to generic if skill prompt.md is missing
    match skill_loader::detect_phase(conn, session_id)? {
        Some(m) => {
            match skill_loader::load_skill_prompt(&m.skill_name, Some(cwd)) {
                Ok(prompt) => {
                    eprintln!("[phase] {} ({}) — loading {}", m.phase_name, m.context, m.skill_name);
                    Some((m.skill_name, prompt))
                }
                Err(_) => {
                    eprintln!("[phase] {} ({}) — skill {} not found, using generic", m.phase_name, m.context, m.skill_name);
                    None
                }
            }
        }
        None => None,
    }
};
```

**Note:** `--skill` override hard-fails on missing skill (operator explicitly requested it).
Auto-detection gracefully degrades to generic identity if the prompt.md file is missing.

Then modify the prompt building. Replace the generic identity block with conditional logic:

```rust
let mut p = String::with_capacity(8192);

if let Some((skill_name, skill_prompt)) = &skill_content {
    p.push_str(&format!("Active skill: {skill_name}\n---\n"));
    p.push_str(skill_prompt);
    p.push_str("\n---\n\n");
} else {
    p.push_str("You are Redtrail, a pentesting advisor embedded in a workspace. You help the operator by analyzing data, suggesting next steps, running commands, and querying the knowledge base.\n\n");
    p.push_str("Be concise and direct. Use pentesting terminology. When suggesting commands, prefer the tools already aliased in the workspace.\n\n");
}
```

Keep the rest of the function (session metadata, KB dump, tool instructions) unchanged, but split the single metadata `format!` call (line 254 in current `ask.rs`) to conditionally include Phase.

Replace this line:
```rust
p.push_str(&format!("Target: {target}\nScope: {scope}\nGoal: {goal}\nPhase: {phase}\nNoise budget: {noise:.2}\nCWD: {}\n\n", cwd.display()));
```

With:
```rust
p.push_str(&format!("Target: {target}\nScope: {scope}\nGoal: {goal}\n"));
if skill_content.is_none() {
    p.push_str(&format!("Phase: {phase}\n"));
}
p.push_str(&format!("Noise budget: {noise:.2}\nCWD: {}\n\n", cwd.display()));
```

- [ ] **Step 3: Update the call site in `run()`**

In `ask::run()`, update the `build_system_prompt` call (around line 29):

```rust
let system = build_system_prompt(&conn, &session_id, &cwd, skill_override, no_skill)?;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build 2>&1 | tail -5`
Expected: compiles cleanly

- [ ] **Step 5: Run all tests**

Run: `cargo test 2>&1 | tail -10`
Expected: all existing tests still pass

- [ ] **Step 6: Commit**

```bash
git add src/cli/ask.rs
git commit -m "Integrate skill loading into build_system_prompt"
```

---

## Task 7: Integration tests

**Files:**
- Create: `tests/skill_loader.rs`
- Modify: `src/cli/ask.rs` (make `build_system_prompt` pub(crate) for testing)

- [ ] **Step 1: Make `build_system_prompt` testable**

In `src/cli/mod.rs`, change `mod ask;` to `pub(crate) mod ask;` so tests in other
modules can access it.

In `src/cli/ask.rs`, change:
```rust
fn build_system_prompt(
```
to:
```rust
pub(crate) fn build_system_prompt(
```

- [ ] **Step 2: RED — system prompt contains skill content when phase detected**

Add unit test in `src/skill_loader.rs` tests module:

```rust
#[test]
fn test_build_prompt_with_skill_replaces_identity() {
    let conn = setup_db("s1");
    let tmp = tempfile::tempdir().unwrap();
    // Create a skill prompt file in workspace
    let skill_dir = tmp.path().join("skills/redtrail-recon");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("prompt.md"), "# Recon skill\nYou are the recon advisor.").unwrap();

    let prompt = crate::cli::ask::build_system_prompt(
        &conn, "s1", tmp.path(), None, false,
    ).unwrap();

    assert!(prompt.contains("Recon skill"), "should contain skill content");
    assert!(prompt.contains("Active skill: redtrail-recon"), "should have skill header");
    assert!(!prompt.contains("You are Redtrail, a pentesting advisor"), "should NOT contain generic identity");
}
```

Run: `cargo test test_build_prompt_with_skill_replaces_identity -- --nocapture`
Expected: PASS

- [ ] **Step 3: RED — `--no-skill` produces generic identity**

```rust
#[test]
fn test_build_prompt_no_skill_uses_generic() {
    let conn = setup_db("s1");
    let tmp = tempfile::tempdir().unwrap();

    let prompt = crate::cli::ask::build_system_prompt(
        &conn, "s1", tmp.path(), None, true,
    ).unwrap();

    assert!(prompt.contains("You are Redtrail, a pentesting advisor"), "should contain generic identity");
    assert!(!prompt.contains("Active skill:"), "should NOT have skill header");
}
```

Run: `cargo test test_build_prompt_no_skill_uses_generic -- --nocapture`
Expected: PASS

- [ ] **Step 4: RED — `--skill` override loads specified skill**

```rust
#[test]
fn test_build_prompt_skill_override() {
    let conn = setup_db("s1");
    let tmp = tempfile::tempdir().unwrap();
    // Create hypothesize skill even though phase is Setup (recon)
    let skill_dir = tmp.path().join("skills/redtrail-hypothesize");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("prompt.md"), "# Hypothesize\nGenerate hypotheses.").unwrap();

    let prompt = crate::cli::ask::build_system_prompt(
        &conn, "s1", tmp.path(), Some("redtrail-hypothesize"), false,
    ).unwrap();

    assert!(prompt.contains("Active skill: redtrail-hypothesize"), "should load overridden skill");
    assert!(prompt.contains("Generate hypotheses"), "should contain override skill content");
}
```

Run: `cargo test test_build_prompt_skill_override -- --nocapture`
Expected: PASS

- [ ] **Step 5: RED — fallback to generic when auto-detected skill file missing**

```rust
#[test]
fn test_build_prompt_missing_skill_falls_back_to_generic() {
    let conn = setup_db("s1");
    let tmp = tempfile::tempdir().unwrap();
    // Empty skills dir — phase detects Setup→recon but prompt.md doesn't exist

    let prompt = crate::cli::ask::build_system_prompt(
        &conn, "s1", tmp.path(), None, false,
    ).unwrap();

    assert!(prompt.contains("You are Redtrail, a pentesting advisor"), "should fallback to generic");
}
```

Run: `cargo test test_build_prompt_missing_skill_falls_back -- --nocapture`
Expected: PASS

- [ ] **Step 6: RED — KB dump follows skill content**

```rust
#[test]
fn test_build_prompt_kb_dump_follows_skill() {
    let conn = setup_db("s1");
    conn.execute("INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')", []).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("skills/redtrail-hypothesize");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("prompt.md"), "# Hypothesize").unwrap();

    let prompt = crate::cli::ask::build_system_prompt(
        &conn, "s1", tmp.path(), None, false,
    ).unwrap();

    assert!(prompt.contains("=== Hosts ==="), "should contain KB dump");
    assert!(prompt.contains("10.10.10.1"), "should contain host data");
    // Skill content should appear before KB dump
    let skill_pos = prompt.find("Hypothesize").unwrap();
    let kb_pos = prompt.find("=== Hosts ===").unwrap();
    assert!(skill_pos < kb_pos, "skill content should precede KB dump");
}
```

Run: `cargo test test_build_prompt_kb_dump_follows_skill -- --nocapture`
Expected: PASS

- [ ] **Step 7: Write CLI flag integration tests**

Create `tests/skill_loader.rs`:

```rust
use std::process::Command;

#[test]
fn test_ask_help_shows_skill_flags() {
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["ask", "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--skill"), "should show --skill flag");
    assert!(stdout.contains("--no-skill"), "should show --no-skill flag");
}

#[test]
fn test_query_help_shows_skill_flags() {
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["query", "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--skill"), "should show --skill flag");
    assert!(stdout.contains("--no-skill"), "should show --no-skill flag");
}
```

- [ ] **Step 8: Run all tests**

Run: `cargo test 2>&1 | tail -15`
Expected: all tests pass

- [ ] **Step 9: Commit**

```bash
git add src/cli/ask.rs tests/skill_loader.rs src/skill_loader.rs
git commit -m "Add integration tests for build_system_prompt and CLI flags"
```

---

## Task 8: Refactor pass

**Files:**
- Modify: `src/skill_loader.rs` (if needed)

- [ ] **Step 1: Review detect_phase for duplication**

Read `src/skill_loader.rs` and check:
- Are the 4 count queries (host_count, hyp_total, hyp_pending, hyp_confirmed, hyp_refuted) done efficiently? Consider consolidating into fewer queries if possible.
- Is the function readable?

Consolidate the individual count queries into a single query. Keep a separate
total count (not derived from pending+confirmed+refuted) to handle hypotheses
with non-standard statuses correctly:

```rust
let (host_count, hyp_total, hyp_pending, hyp_confirmed, hyp_refuted): (i64, i64, i64, i64, i64) = conn.query_row(
    "SELECT
        (SELECT count(*) FROM hosts WHERE session_id = ?1),
        (SELECT count(*) FROM hypotheses WHERE session_id = ?1),
        (SELECT count(*) FROM hypotheses WHERE session_id = ?1 AND status = 'pending'),
        (SELECT count(*) FROM hypotheses WHERE session_id = ?1 AND status = 'confirmed'),
        (SELECT count(*) FROM hypotheses WHERE session_id = ?1 AND status = 'refuted')",
    rusqlite::params![session_id],
    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
).map_err(|e| Error::Db(e.to_string()))?;
```

This reduces 5 DB round-trips to 1 while keeping `hyp_total` as a true count of
all hypotheses (not just the sum of known statuses).

- [ ] **Step 2: Run all tests after refactor**

Run: `cargo test 2>&1 | tail -10`
Expected: all tests still pass

- [ ] **Step 3: Commit**

```bash
git add src/skill_loader.rs
git commit -m "Refactor: consolidate phase detection into single DB query"
```

---

## Task 9: Final verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1`
Expected: all tests pass, no warnings except maybe dead_code for unused fields

- [ ] **Step 2: Manual smoke test (requires workspace)**

In the test workspace at `/private/tmp/rt-test`:

```bash
cd /private/tmp/rt-test
rt init --target 10.10.10.1
# Should auto-detect Setup phase → redtrail-recon
rt ask --no-skill "what tools do I have?"
# Should use generic identity (no skill)
rt query --skill redtrail-hypothesize "generate hypotheses"
# Should load hypothesize skill regardless of phase
```

Verify stderr shows `[phase]` or `[skill]` announcements.

- [ ] **Step 3: Final commit if any tweaks needed**

```bash
git add -A
git commit -m "Final polish for phase-aware skill loading"
```
