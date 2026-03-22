# Plan: Global Database & Config-in-DB

> Source PRD: `docs/prd/2026-03-22-global-db-config.md`
> Design spec: `docs/superpowers/specs/2026-03-22-global-db-config-design.md`

## Architectural decisions

- **DB location**: `~/.redtrail/redtrail.db`, single global SQLite DB, auto-created on first invocation
- **Schema**: sessions gain `workspace_path TEXT NOT NULL` and `active INTEGER DEFAULT 1`. New `global_config(key, value)` and `session_config(session_id, key, value)` tables. Partial unique index enforces one active session per workspace.
- **Resolution interface**:
  - `resolve(cwd: &Path) -> Result<SessionContext>` — returns `{ conn, session_id, workspace_path, config }` for commands that need a session
  - `resolve_global() -> Result<GlobalContext>` — returns `{ conn }` for commands that only need the DB
  - `GlobalContext::find_session(cwd) -> Option<(session_id, workspace_path)>` — non-failing session lookup
- **Config resolution**: hardcoded defaults → `global_config` table → `session_config` table. All values stored as TEXT with type-aware parsing (bool, int, float, JSON arrays).
- **Config struct**: `Config` and its sub-structs (`GeneralConfig`, `ScopeConfig`, etc.) stay as-is. Only the backing store changes.
- **Session lifecycle**: one active session per workspace_path. `rt init` creates, `rt session new` rotates, `rt session activate` switches.
- **TDD approach**: each phase writes tests first (red), implements (green), then refactors. Tests use in-memory SQLite with the new schema.

---

## Phase 1: Global DB + Schema

**User stories**: 1, 2, 22

### What to build

The new foundation: global DB at `~/.redtrail/redtrail.db` with the updated schema. The `resolve_global()` function that opens/creates the DB. Rewrite `rt init` to create a session in the global DB with `workspace_path = $CWD` — no filesystem changes, no `.redtrail/` directory.

This phase proves the global DB works end-to-end: you can `rt init` in any directory and a session appears in the global DB.

### Acceptance criteria

- [ ] `~/.redtrail/redtrail.db` is created on first `rt` invocation
- [ ] Schema includes `sessions` table with `workspace_path` and `active` columns
- [ ] Schema includes `global_config` and `session_config` tables
- [ ] Partial unique index `idx_one_active_per_workspace` exists and prevents two active sessions for the same path
- [ ] `resolve_global()` returns a `GlobalContext` with an open connection
- [ ] `rt init --target 10.10.10.1` creates a session row with `workspace_path = $CWD`, `active = 1`
- [ ] `rt init` in an already-initialized directory errors with a helpful message
- [ ] No `.redtrail/` directory is created anywhere
- [ ] `autonomy` column is not present in the sessions table
- [ ] Tests: schema creation, session insert, duplicate active prevention via index, `resolve_global` returns valid connection

---

## Phase 2: Session Resolution

**User stories**: 3, 5

### What to build

The `resolve(cwd)` function that walks CWD upward to find an active session, builds the `SessionContext` with connection + session_id + workspace_path + config. Replace `resolve_session()` in `cli/mod.rs` to use the new resolution. Delete `workspace.rs`. All existing commands (status, kb, hypothesis, etc.) work against the global DB through the new resolution.

This phase proves that the entire CLI works with the global DB — every command that previously needed `find_workspace()` now uses `resolve()`.

### Acceptance criteria

- [ ] `resolve(cwd)` returns `SessionContext { conn, session_id, workspace_path, config }` for a directory with an active session
- [ ] `resolve(cwd)` walks up parent directories and finds sessions registered to ancestor paths
- [ ] `resolve(cwd)` returns `Error::NoActiveSession` when no session matches
- [ ] `resolve_session()` in `cli/mod.rs` opens the global DB and delegates to `resolve()`
- [ ] All existing commands (`rt status`, `rt kb`, `rt hypothesis`, etc.) work unchanged through the new resolution
- [ ] `workspace.rs` is deleted
- [ ] Tests: exact CWD match, subdirectory resolution, no-match error, multiple sessions same path (only active returned)

---

## Phase 3: Config-in-DB

**User stories**: 12, 13, 14, 15, 16, 17

### What to build

DB-backed config resolution: `Config::resolved(conn, session_id)` reads `global_config` and `session_config` tables and builds the `Config` struct with three-tier merge (defaults → global → session). Rewrite `rt config set/get/list` to read/write DB tables. `rt config set` writes to `session_config`, `rt config set --global` writes to `global_config`. Outside a workspace, `set` falls back to global. Delete TOML loading/merging from `config.rs`.

### Acceptance criteria

- [ ] `Config::resolved(conn, session_id)` returns correct config with three-tier merge
- [ ] Global config overrides defaults, session config overrides global
- [ ] Type parsing works: bool (`"true"`/`"false"`), integers, JSON arrays
- [ ] Unknown keys are ignored without error
- [ ] `rt config set general.llm_model <value>` writes to `session_config`
- [ ] `rt config set --global general.llm_model <value>` writes to `global_config`
- [ ] `rt config set` outside a workspace falls back to `global_config`
- [ ] `rt config get <key>` shows the resolved value
- [ ] `rt config list` shows the full resolved config
- [ ] TOML loading methods (`load_global`, `load_workspace`, `merge_workspace`) are deleted from `config.rs`
- [ ] Tests: defaults-only, global override, session override wins, type parsing (bool, int, JSON array), unknown key ignored, set/get round-trip

---

## Phase 4: Session Lifecycle

**User stories**: 4, 5, 6, 7, 8

### What to build

New session management commands: `rt session new` (deactivates current, creates fresh active session), `rt session activate <name|id>` (swaps active flag), `rt session list` (lists all sessions for CWD), `rt session list --all` (lists across all workspaces). Add `New` and `Activate` variants to `SessionCommands` enum. Add `deactivate_session()` and `activate_session()` to the `SessionOps` trait.

### Acceptance criteria

- [ ] `rt session new --target X` deactivates the current session and creates a new active one for the same workspace_path
- [ ] `rt session activate <name>` validates same workspace_path, swaps active flag
- [ ] `rt session activate` with a session from a different workspace errors
- [ ] `rt session list` shows all sessions (active and archived) for the current CWD
- [ ] `rt session list --all` shows sessions across all workspaces
- [ ] Active session is marked in the list output
- [ ] Partial unique index prevents activating two sessions for the same workspace (DB-level guarantee)
- [ ] Tests: new session deactivates old, activate swaps correctly, list filters by workspace, list --all returns all, cross-workspace activate rejected

---

## Phase 5: Environment Variables

**User stories**: 9, 10, 11, 18, 19

### What to build

Rewrite `rt env` to generate pentesting environment variables from the session row and resolved config. Exports: `RT_SESSION`, `RT_WORKSPACE`, `TARGET`, `RHOST`, `SCOPE`, `LHOST` (via `ip route get $TARGET`), `LPORT` (from config key `env.lport`, default 4444). Tool aliases generated from resolved `tools.aliases` config. Command aliases (kb, st, theory, etc.) kept as hardcoded. Prompt modification and `rt_deactivate` function preserved. Rewrite `rt deactivate` to use global DB for config resolution.

### Acceptance criteria

- [ ] `rt env` output contains `export TARGET='<value>'` from session row
- [ ] `rt env` output contains `export RHOST='<value>'` matching TARGET
- [ ] `rt env` output contains `export SCOPE='<value>'` from session row
- [ ] `rt env` output contains `LHOST=$(ip route get ...)` detection command
- [ ] `rt env` output contains `export LPORT=4444` (or configured value)
- [ ] `rt env` output contains tool aliases from resolved config
- [ ] `rt env` output contains hardcoded command aliases (kb, st, theory, etc.)
- [ ] `rt env` output contains prompt modification (`_rt_precmd`)
- [ ] `rt env` output contains `rt_deactivate` function that unsets all vars and aliases
- [ ] `rt deactivate` works with global DB
- [ ] Tests: env output contains expected exports, aliases, prompt, deactivate function; LPORT respects config override

---

## Phase 6: Cleanup

**User stories**: 20, 21

### What to build

Final cleanup: rewrite `rt setup` wizard to write to `global_config` table instead of `~/.redtrail/config.toml`. Update `rt sql` to open the global DB path. Remove `toml` crate dependency if no longer used. Remove any remaining references to `workspace.rs`, `.redtrail/` directory creation, or TOML config file paths. Update `spawn.rs` extraction pipeline DB path.

### Acceptance criteria

- [ ] `rt setup` wizard writes config values to `global_config` table
- [ ] `rt sql` opens `~/.redtrail/redtrail.db`
- [ ] `toml` crate removed from `Cargo.toml` (if no other usage)
- [ ] No references to `.redtrail/config.toml` or `.redtrail/aliases.sh` remain in the codebase
- [ ] No references to `workspace::find_workspace` or `workspace::db_path` remain
- [ ] Extraction pipeline (`spawn.rs`) uses global DB path
- [ ] Tests: setup wizard writes to DB, sql opens correct path
