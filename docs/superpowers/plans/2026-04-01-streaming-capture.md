# Streaming Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the capture lifecycle into `capture start` / `capture finish` so the tee process can stream output to the DB every 1 second, enabling real-time visibility into long-running commands.

**Architecture:** Shell hooks call `capture start` synchronously in preexec (inserts minimal DB row, returns command ID). Tee process takes the command ID, opens its own DB connection, and flushes redacted output every 1s. Precmd waits for tee to exit, then backgrounds `capture finish` which fills in git context, does a final redaction pass, compresses if needed, and updates FTS.

**Tech Stack:** Rust, SQLite (WAL mode), shell hooks (zsh + bash)

**Spec:** `docs/superpowers/specs/2026-04-01-streaming-capture-design.md`

**Spec deviation:** The spec's "New Flow" section lists `git_repo, git_branch, env_snapshot` at `capture start` time, but the spec's own performance budget section shows `capture start` has no room for git subprocess calls (10-30ms). This plan defers git context and env snapshot to `capture finish` (which runs backgrounded), keeping `capture start` under the 15ms budget. This is the correct tradeoff — the spec is internally contradictory on this point.

---

## File Structure

| File | Responsibility | Change Type |
|---|---|---|
| `src/core/db.rs` | DB schema, migrations, insert/update/finish functions | Modify |
| `src/cmd/capture.rs` | `capture start` and `capture finish` CLI handlers | Rewrite |
| `src/cli.rs` | CLI arg parsing — Capture subcommand group | Modify |
| `src/core/tee.rs` | PTY relay + DB streaming flush + secret redaction | Major modify |
| `src/cmd/tee.rs` | Tee CLI args wrapper | Modify |
| `src/cmd/init.rs` | Shell hook scripts (zsh + bash) | Rewrite hooks |
| `tests/init_test.rs` | Unit tests for hook scripts | Update |
| `eval/tests/capture-basic.sh` | Live test: basic capture | Update |
| `eval/tests/capture-basic-bash.sh` | Live test: basic bash capture | Update |
| `eval/tests/capture-stdout.sh` | Live test: stdout capture | Update |
| `eval/tests/capture-stdout-bash.sh` | Live test: bash stdout capture | Update |
| `eval/tests/capture-streaming.sh` | Live test: streaming mid-execution check | Create |
| `eval/tests/capture-streaming-redact.sh` | Live test: secrets redacted during streaming | Create |
| `eval/tests/capture-streaming-block.sh` | Live test: on_detect=block deletes row | Create |
| `eval/tests/capture-streaming-warn.sh` | Live test: on_detect=warn single warning | Create |
| `eval/tests/capture-rapid-commands.sh` | Live test: rapid sequential commands | Create |
| `eval/tests/capture-orphan-cleanup.sh` | Live test: stale running commands cleaned | Create |
| `tests/capture_cli_test.rs` | Unit tests for capture CLI | Update |
| `tests/tee_cli_test.rs` | Unit tests for tee CLI | Update |
| `tests/db_test.rs` | DB function unit tests | Update (add, not overwrite) |

---

## Task 1: DB Schema — Add `status` column and new functions

**Files:**
- Modify: `src/core/db.rs:4-156` (SCHEMA const + migrations)
- Modify: `src/core/db.rs:245-268` (NewCommand struct)
- Test: `tests/db_test.rs` (existing file — add new tests, DO NOT overwrite)

**Note:** `tests/db_test.rs` already exists with schema validation tests. Append to it.

**Note:** `insert_command`, `insert_command_compressed`, and `insert_command_redacted_compressed` do not include a `status` column in their INSERT statements. After the migration adds `status TEXT NOT NULL DEFAULT 'finished'`, they will correctly default to `'finished'` via the DEFAULT clause. No changes needed to those functions.

### Steps

- [ ] **Step 1: Write failing test for `status` column migration**

Add to `tests/db_test.rs`:

```rust
use redtrail::core::db;

#[test]
fn commands_table_has_status_column() {
    let conn = db::open_in_memory().unwrap();
    let has_status: bool = conn
        .prepare("PRAGMA table_info(commands)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .any(|col| col.as_deref() == Ok("status"));
    assert!(has_status, "commands table should have status column");
}

#[test]
fn new_command_defaults_to_finished() {
    let conn = db::open_in_memory().unwrap();
    let id = db::insert_command(&conn, &db::NewCommand {
        session_id: "test-session",
        command_raw: "echo hello",
        source: "human",
        timestamp_start: 1000,
        ..Default::default()
    }).unwrap();
    let status: String = conn
        .query_row("SELECT status FROM commands WHERE id = ?1", [&id], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "finished");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test commands_table_has_status_column -- --nocapture`
Expected: FAIL — `status` column does not exist yet

- [ ] **Step 3: Add `status` column to schema and migration**

In `src/core/db.rs`, add to the SCHEMA const inside the commands CREATE TABLE:

```sql
    status TEXT NOT NULL DEFAULT 'finished',
```

Add after the existing indexes in the SCHEMA const:

```sql
CREATE INDEX IF NOT EXISTS idx_commands_status ON commands(status) WHERE status = 'running';
```

Add a new migration function `migrate_status_column` (same pattern as `migrate_agent_columns`):

```rust
fn migrate_status_column(conn: &Connection) -> Result<(), Error> {
    let has_status: bool = conn
        .prepare("PRAGMA table_info(commands)")
        .map_err(|e| Error::Db(e.to_string()))?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| Error::Db(e.to_string()))?
        .any(|col| col.as_deref() == Ok("status"));

    if !has_status {
        conn.execute_batch(
            "ALTER TABLE commands ADD COLUMN status TEXT NOT NULL DEFAULT 'finished';
             CREATE INDEX IF NOT EXISTS idx_commands_status ON commands(status) WHERE status = 'running';"
        )
        .map_err(|e| Error::Db(e.to_string()))?;
    }
    Ok(())
}
```

Call it from `init()` after `migrate_compressed_columns(conn)?;`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test commands_table_has_status -- --nocapture`
Expected: PASS

- [ ] **Step 5: Write failing tests for `insert_command_start`, `update_command_output`, `finish_command`**

Add to `tests/db_test.rs`:

```rust
#[test]
fn insert_command_start_creates_running_row() {
    let conn = db::open_in_memory().unwrap();
    db::create_session(&conn, &db::NewSession {
        cwd_initial: None, hostname: None, shell: None, source: "human",
    }).unwrap();
    let id = db::insert_command_start(&conn, &db::NewCommandStart {
        session_id: "test",
        command_raw: "rails s",
        command_binary: Some("rails"),
        command_subcommand: Some("s"),
        command_args: None,
        command_flags: None,
        cwd: Some("/app"),
        shell: Some("zsh"),
        hostname: Some("localhost"),
        source: "human",
        redacted: false,
    }).unwrap();

    let (status, exit_code): (String, Option<i32>) = conn
        .query_row(
            "SELECT status, exit_code FROM commands WHERE id = ?1",
            [&id], |r| Ok((r.get(0)?, r.get(1)?))
        ).unwrap();
    assert_eq!(status, "running");
    assert_eq!(exit_code, None);
}

#[test]
fn update_command_output_writes_stdout() {
    let conn = db::open_in_memory().unwrap();
    let id = db::insert_command_start(&conn, &db::NewCommandStart {
        session_id: "test",
        command_raw: "echo hi",
        source: "human",
        ..Default::default()
    }).unwrap();

    db::update_command_output(&conn, &id, "hello world", "", false, false).unwrap();

    let stdout: Option<String> = conn
        .query_row("SELECT stdout FROM commands WHERE id = ?1", [&id], |r| r.get(0))
        .unwrap();
    assert_eq!(stdout.as_deref(), Some("hello world"));
}

#[test]
fn finish_command_sets_status_and_exit_code() {
    let conn = db::open_in_memory().unwrap();
    let id = db::insert_command_start(&conn, &db::NewCommandStart {
        session_id: "test",
        command_raw: "echo hi",
        source: "human",
        ..Default::default()
    }).unwrap();

    db::update_command_output(&conn, &id, "output", "", false, false).unwrap();

    db::finish_command(&conn, &db::FinishCommand {
        command_id: &id,
        exit_code: Some(0),
        git_repo: Some("/repo"),
        git_branch: Some("main"),
        env_snapshot: Some("{}"),
        stdout: None,  // keep tee's stdout
        stderr: None,
    }).unwrap();

    let (status, exit_code, git_repo): (String, Option<i32>, Option<String>) = conn
        .query_row(
            "SELECT status, exit_code, git_repo FROM commands WHERE id = ?1",
            [&id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        ).unwrap();
    assert_eq!(status, "finished");
    assert_eq!(exit_code, Some(0));
    assert_eq!(git_repo.as_deref(), Some("/repo"));
}
```

- [ ] **Step 6: Run tests to verify they fail**

Run: `cargo test insert_command_start -- --nocapture`
Expected: FAIL — functions don't exist yet

- [ ] **Step 7: Implement `NewCommandStart`, `insert_command_start`, `update_command_output`, `FinishCommand`, `finish_command`**

Add to `src/core/db.rs`:

```rust
#[derive(Default)]
pub struct NewCommandStart<'a> {
    pub session_id: &'a str,
    pub command_raw: &'a str,
    pub command_binary: Option<&'a str>,
    pub command_subcommand: Option<&'a str>,
    pub command_args: Option<&'a str>,
    pub command_flags: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub shell: Option<&'a str>,
    pub hostname: Option<&'a str>,
    pub source: &'a str,
    pub redacted: bool,
}

pub fn insert_command_start(conn: &Connection, cmd: &NewCommandStart) -> Result<String, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, command_binary,
         command_subcommand, command_args, command_flags, cwd, shell, hostname, source, redacted, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'running')",
        rusqlite::params![
            id, cmd.session_id, now, cmd.command_raw, cmd.command_binary,
            cmd.command_subcommand, cmd.command_args, cmd.command_flags,
            cmd.cwd, cmd.shell, cmd.hostname, cmd.source, cmd.redacted,
        ],
    ).map_err(|e| Error::Db(e.to_string()))?;

    // Update session counters
    conn.execute(
        "UPDATE sessions SET command_count = command_count + 1 WHERE id = ?1",
        [cmd.session_id],
    ).ok();

    Ok(id)
}

pub fn update_command_output(
    conn: &Connection,
    command_id: &str,
    stdout: &str,
    stderr: &str,
    stdout_truncated: bool,
    stderr_truncated: bool,
) -> Result<(), Error> {
    conn.execute(
        "UPDATE commands SET stdout = ?1, stderr = ?2,
         stdout_truncated = ?3, stderr_truncated = ?4
         WHERE id = ?5 AND status = 'running'",
        rusqlite::params![stdout, stderr, stdout_truncated, stderr_truncated, command_id],
    ).map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub struct FinishCommand<'a> {
    pub command_id: &'a str,
    pub exit_code: Option<i32>,
    pub git_repo: Option<&'a str>,
    pub git_branch: Option<&'a str>,
    pub env_snapshot: Option<&'a str>,
    pub stdout: Option<&'a str>,
    pub stderr: Option<&'a str>,
}

pub fn finish_command(conn: &Connection, cmd: &FinishCommand) -> Result<(), Error> {
    conn.execute(
        "UPDATE commands SET exit_code = ?1, timestamp_end = unixepoch(),
         git_repo = ?2, git_branch = ?3, env_snapshot = ?4,
         stdout = COALESCE(?5, stdout), stderr = COALESCE(?6, stderr),
         status = 'finished'
         WHERE id = ?7",
        rusqlite::params![
            cmd.exit_code, cmd.git_repo, cmd.git_branch, cmd.env_snapshot,
            cmd.stdout, cmd.stderr, cmd.command_id,
        ],
    ).map_err(|e| Error::Db(e.to_string()))?;

    // Sync FTS index
    let rowid_result: Result<i64, _> = conn.query_row(
        "SELECT rowid FROM commands WHERE id = ?1", [cmd.command_id], |r| r.get(0)
    );
    if let Ok(rowid) = rowid_result {
        // Get final content for FTS
        let (raw, stdout, stderr): (String, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT command_raw, stdout, stderr FROM commands WHERE id = ?1",
                [cmd.command_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            ).map_err(|e| Error::Db(e.to_string()))?;
        conn.execute(
            "INSERT INTO commands_fts(rowid, command_raw, stdout, stderr) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![rowid, raw, stdout, stderr],
        ).map_err(|e| Error::Db(e.to_string()))?;
    }

    // Update session error_count
    if cmd.exit_code.is_some_and(|c| c != 0) {
        // Get session_id from the command row
        if let Ok(session_id) = conn.query_row::<String, _, _>(
            "SELECT session_id FROM commands WHERE id = ?1", [cmd.command_id], |r| r.get(0)
        ) {
            conn.execute(
                "UPDATE sessions SET error_count = error_count + 1 WHERE id = ?1",
                [&session_id],
            ).ok();
        }
    }

    Ok(())
}

/// Delete a command row (used by tee in on_detect=block mode).
pub fn delete_command(conn: &Connection, command_id: &str) -> Result<(), Error> {
    conn.execute("DELETE FROM commands WHERE id = ?1", [command_id])
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

/// Mark stale running commands as orphaned.
pub fn cleanup_orphaned_commands(conn: &Connection, session_id: &str) -> Result<usize, Error> {
    let affected = conn.execute(
        "UPDATE commands SET status = 'orphaned'
         WHERE status = 'running' AND session_id = ?1
         AND timestamp_start < unixepoch() - 86400",
        [session_id],
    ).map_err(|e| Error::Db(e.to_string()))?;
    Ok(affected)
}
```

- [ ] **Step 8: Write tests for `delete_command` and `cleanup_orphaned_commands`**

Add to `tests/db_test.rs`:

```rust
#[test]
fn delete_command_removes_row() {
    let conn = db::open_in_memory().unwrap();
    let id = db::insert_command_start(&conn, &db::NewCommandStart {
        session_id: "test",
        command_raw: "echo secret",
        source: "human",
        ..Default::default()
    }).unwrap();

    db::delete_command(&conn, &id).unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM commands WHERE id = ?1", [&id], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn cleanup_orphaned_commands_marks_stale_running() {
    let conn = db::open_in_memory().unwrap();
    // Insert a "running" command with old timestamp
    let id = uuid::Uuid::new_v4().to_string();
    let old_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64 - 100_000; // >24h ago
    conn.execute(
        "INSERT INTO commands (id, session_id, timestamp_start, command_raw, source, status)
         VALUES (?1, 'sess1', ?2, 'old cmd', 'human', 'running')",
        rusqlite::params![id, old_ts],
    ).unwrap();

    let affected = db::cleanup_orphaned_commands(&conn, "sess1").unwrap();
    assert_eq!(affected, 1);

    let status: String = conn
        .query_row("SELECT status FROM commands WHERE id = ?1", [&id], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "orphaned");
}
```

- [ ] **Step 9: Run all tests**

Run: `cargo test`
Expected: All new DB tests pass. Existing tests still pass.

- [ ] **Step 10: Commit**

```bash
git add src/core/db.rs tests/db_test.rs
git commit -m "feat: add streaming capture DB layer — status column, insert_command_start, update_command_output, finish_command"
```

---

## Task 2: CLI — Replace `capture` with `capture start` / `capture finish`

**Files:**
- Rewrite: `src/cmd/capture.rs`
- Modify: `src/cli.rs:82-104` (Commands::Capture) and `src/cli.rs:297-313` (match arm)

### Steps

- [ ] **Step 1: Rewrite `src/cli.rs` — replace Capture variant with subcommand group**

Replace the `Commands::Capture` variant and its match arm. The new CLI structure:

```rust
/// Record command execution (called by shell hooks)
#[command(hide = true)]
Capture {
    #[command(subcommand)]
    action: CaptureAction,
},
```

Add the `CaptureAction` enum:

```rust
#[derive(Subcommand)]
enum CaptureAction {
    /// Create a running command record (preexec)
    Start {
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        command: String,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long)]
        shell: Option<String>,
        #[arg(long)]
        hostname: Option<String>,
    },
    /// Finalize a command record (precmd)
    Finish {
        #[arg(long)]
        command_id: String,
        #[arg(long)]
        exit_code: Option<i32>,
        #[arg(long)]
        cwd: Option<String>,
    },
}
```

Replace the match arm for `Commands::Capture` in `run()`:

```rust
Commands::Capture { action } => {
    let config = redtrail::config::Config::load(&config_path()).unwrap_or_default();
    let conn = open_db()?;
    match action {
        CaptureAction::Start { session_id, command, cwd, shell, hostname } => {
            cmd::capture::start(&conn, &cmd::capture::StartArgs {
                session_id: &session_id,
                command: &command,
                cwd: cwd.as_deref(),
                shell: shell.as_deref(),
                hostname: hostname.as_deref(),
                config: &config,
            })
        }
        CaptureAction::Finish { command_id, exit_code, cwd } => {
            cmd::capture::finish(&conn, &cmd::capture::FinishArgs {
                command_id: &command_id,
                exit_code,
                cwd: cwd.as_deref(),
                config: &config,
            })
        }
    }
}
```

- [ ] **Step 2: Rewrite `src/cmd/capture.rs`**

```rust
use crate::core::capture;
use crate::core::db;
use crate::error::Error;
use rusqlite::Connection;

pub struct StartArgs<'a> {
    pub session_id: &'a str,
    pub command: &'a str,
    pub cwd: Option<&'a str>,
    pub shell: Option<&'a str>,
    pub hostname: Option<&'a str>,
    pub config: &'a crate::config::Config,
}

pub fn start(conn: &Connection, args: &StartArgs) -> Result<(), Error> {
    if !args.config.capture.enabled {
        return Ok(());  // empty stdout = no ID = shell hook skips
    }

    let parsed = capture::parse_command(args.command);

    if capture::is_blacklisted(&parsed.binary, &args.config.capture.blacklist_commands) {
        return Ok(());
    }

    let env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let source = capture::detect_source(&env, None);

    // Secret redaction on command_raw
    use crate::config::OnDetect;
    use crate::core::secrets::engine::{load_custom_patterns, redact_with_custom_patterns};

    let custom_patterns = args.config.secrets.patterns_file
        .as_deref()
        .map(load_custom_patterns)
        .unwrap_or_default();

    let (command_raw, was_redacted) = match args.config.secrets.on_detect {
        OnDetect::Block => {
            let (_, labels) = redact_with_custom_patterns(args.command, &custom_patterns);
            if !labels.is_empty() {
                return Ok(());  // secrets found, don't capture
            }
            (args.command.to_string(), false)
        }
        OnDetect::Redact => {
            let (redacted, labels) = redact_with_custom_patterns(args.command, &custom_patterns);
            let was_redacted = !labels.is_empty();
            (redacted, was_redacted)
        }
        OnDetect::Warn => {
            let (_, labels) = redact_with_custom_patterns(args.command, &custom_patterns);
            if !labels.is_empty() {
                eprintln!("[redtrail] WARN: secrets detected in command ({})", labels.join(", "));
            }
            (args.command.to_string(), !labels.is_empty())
        }
    };

    let args_json = serde_json::to_string(&parsed.args).unwrap_or_default();
    let flags_json = serde_json::to_string(&parsed.flags).unwrap_or_default();

    // Orphan cleanup
    let _ = db::cleanup_orphaned_commands(conn, args.session_id);

    let id = db::insert_command_start(conn, &db::NewCommandStart {
        session_id: args.session_id,
        command_raw: &command_raw,
        command_binary: if parsed.binary.is_empty() { None } else { Some(&parsed.binary) },
        command_subcommand: parsed.subcommand.as_deref(),
        command_args: Some(&args_json),
        command_flags: Some(&flags_json),
        cwd: args.cwd,
        shell: args.shell,
        hostname: args.hostname,
        source,
        redacted: was_redacted,
    })?;

    // Print command ID — shell hook captures this
    print!("{id}");
    Ok(())
}

pub struct FinishArgs<'a> {
    pub command_id: &'a str,
    pub exit_code: Option<i32>,
    pub cwd: Option<&'a str>,
    pub config: &'a crate::config::Config,
}

pub fn finish(conn: &Connection, args: &FinishArgs) -> Result<(), Error> {
    // Check if command row still exists (tee may have deleted it in block mode)
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) FROM commands WHERE id = ?1", [args.command_id], |r| r.get::<_, i64>(0)
    ).map(|c| c > 0).unwrap_or(false);

    if !exists {
        return Ok(());
    }

    // Get git context (safe to do here — this runs backgrounded)
    let git = args.cwd.map(capture::git_context);
    let git_repo = git.as_ref().and_then(|g| g.repo.as_deref());
    let git_branch = git.as_ref().and_then(|g| g.branch.as_deref());

    let env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let env_snap = capture::env_snapshot(&env);

    // Final redaction pass on stdout/stderr (defense-in-depth)
    use crate::config::OnDetect;
    use crate::core::secrets::engine::{load_custom_patterns, redact_with_custom_patterns};

    let custom_patterns = args.config.secrets.patterns_file
        .as_deref()
        .map(load_custom_patterns)
        .unwrap_or_default();

    let (final_stdout, final_stderr) = {
        let row: (Option<String>, Option<String>) = conn.query_row(
            "SELECT stdout, stderr FROM commands WHERE id = ?1",
            [args.command_id], |r| Ok((r.get(0)?, r.get(1)?))
        ).unwrap_or((None, None));

        match args.config.secrets.on_detect {
            OnDetect::Block => {
                let stdout_has_secrets = row.0.as_ref()
                    .is_some_and(|s| !redact_with_custom_patterns(s, &custom_patterns).1.is_empty());
                let stderr_has_secrets = row.1.as_ref()
                    .is_some_and(|s| !redact_with_custom_patterns(s, &custom_patterns).1.is_empty());
                if stdout_has_secrets || stderr_has_secrets {
                    db::delete_command(conn, args.command_id)?;
                    return Ok(());
                }
                (None, None) // keep existing
            }
            OnDetect::Redact => {
                let stdout = row.0.map(|s| redact_with_custom_patterns(&s, &custom_patterns).0);
                let stderr = row.1.map(|s| redact_with_custom_patterns(&s, &custom_patterns).0);
                (stdout, stderr)
            }
            OnDetect::Warn => {
                // Already stored unredacted by tee, just check and warn
                let stdout_labels = row.0.as_ref()
                    .map(|s| redact_with_custom_patterns(s, &custom_patterns).1)
                    .unwrap_or_default();
                let stderr_labels = row.1.as_ref()
                    .map(|s| redact_with_custom_patterns(s, &custom_patterns).1)
                    .unwrap_or_default();
                if !stdout_labels.is_empty() || !stderr_labels.is_empty() {
                    eprintln!("[redtrail] WARN: secrets detected in output");
                }
                (None, None)
            }
        }
    };

    db::finish_command(conn, &db::FinishCommand {
        command_id: args.command_id,
        exit_code: args.exit_code,
        git_repo,
        git_branch,
        env_snapshot: Some(&env_snap),
        stdout: final_stdout.as_deref(),
        stderr: final_stderr.as_deref(),
    })?;

    // Compress if over limit
    let max_bytes = args.config.capture.max_stdout_bytes;
    db::compress_command_output_if_needed(conn, args.command_id, max_bytes)?;

    // Enforce retention
    let _ = db::enforce_retention(conn, args.config.capture.retention_days);

    Ok(())
}
```

- [ ] **Step 3: Add `compress_command_output_if_needed` to `src/core/db.rs`**

```rust
/// Compress stdout/stderr to blob columns if they exceed max_bytes. Called at finalize time.
pub fn compress_command_output_if_needed(
    conn: &Connection,
    command_id: &str,
    max_bytes: usize,
) -> Result<(), Error> {
    let (stdout, stderr): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT stdout, stderr FROM commands WHERE id = ?1",
            [command_id], |r| Ok((r.get(0)?, r.get(1)?))
        ).map_err(|e| Error::Db(e.to_string()))?;

    if let Some(ref s) = stdout {
        if s.len() > max_bytes {
            let compressed = compress_zlib(s);
            conn.execute(
                "UPDATE commands SET stdout = NULL, stdout_compressed = ?1, stdout_truncated = 1 WHERE id = ?2",
                rusqlite::params![compressed, command_id],
            ).map_err(|e| Error::Db(e.to_string()))?;
        }
    }
    if let Some(ref s) = stderr {
        if s.len() > max_bytes {
            let compressed = compress_zlib(s);
            conn.execute(
                "UPDATE commands SET stderr = NULL, stderr_compressed = ?1, stderr_truncated = 1 WHERE id = ?2",
                rusqlite::params![compressed, command_id],
            ).map_err(|e| Error::Db(e.to_string()))?;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Update `tests/capture_cli_test.rs`**

This file tests the capture CLI by calling `redtrail capture --session-id ... --command ...`. Update all such calls to use the new subcommand: `redtrail capture start --session-id ... --command ...`. Similarly update any assertions about the old `--stdout-file` / `--stderr-file` args (removed in new flow). Update finish-related tests to call `redtrail capture finish --command-id ... --exit-code ...`.

Read the file first to identify all test functions that need updating.

- [ ] **Step 5: Verify compilation**

Run: `cargo build`
Expected: Compiles with no errors.

- [ ] **Step 6: Run all unit tests**

Run: `cargo test`
Expected: All tests pass (existing + new DB tests + updated capture CLI tests).

- [ ] **Step 7: Commit**

```bash
git add src/cli.rs src/cmd/capture.rs src/core/db.rs tests/capture_cli_test.rs
git commit -m "feat: replace capture with capture start/finish subcommands"
```

---

## Task 3: Tee — Add DB streaming with 1s flush and secret redaction

**Files:**
- Modify: `src/core/tee.rs` (major rewrite of `run_tee`)
- Modify: `src/cmd/tee.rs` (update args)
- Modify: `src/cli.rs` (update Tee command args)

### Steps

- [ ] **Step 1: Update `TeeConfig` and `TeeArgs` — `command_id` replaces `session`**

In `src/core/tee.rs`, change `TeeConfig`:

```rust
pub struct TeeConfig {
    pub command_id: String,
    pub shell_pid: String,
    pub ctl_fifo: String,
    pub max_bytes: usize,
}
```

In `src/cmd/tee.rs`, update `TeeArgs` to use `command_id`:

```rust
pub struct TeeArgs<'a> {
    pub command_id: &'a str,
    pub shell_pid: &'a str,
    pub ctl_fifo: &'a str,
    pub max_bytes: Option<usize>,
}
```

And the `run` fn:

```rust
pub fn run(args: &TeeArgs) -> Result<(), Error> {
    tee::run_tee(&tee::TeeConfig {
        command_id: args.command_id.to_string(),
        shell_pid: args.shell_pid.to_string(),
        ctl_fifo: args.ctl_fifo.to_string(),
        max_bytes: args.max_bytes.unwrap_or(MAX_STDOUT_BYTES),
    })
}
```

In `src/cli.rs`, update the Tee command:

```rust
Tee {
    #[arg(long)]
    command_id: String,
    #[arg(long)]
    shell_pid: String,
    #[arg(long)]
    ctl_fifo: String,
    #[arg(long)]
    max_bytes: Option<usize>,
},
```

And the match arm:

```rust
Commands::Tee { command_id, shell_pid, ctl_fifo, max_bytes } => {
    cmd::tee::run(&cmd::tee::TeeArgs {
        command_id: &command_id,
        shell_pid: &shell_pid,
        ctl_fifo: &ctl_fifo,
        max_bytes,
    })
}
```

- [ ] **Step 2: Add DB flush + redaction to `run_tee` in `src/core/tee.rs`**

The key changes to `run_tee`:

1. Open DB connection at startup (after PTY setup, before relay loop)
2. Load config and custom patterns for redaction
3. Add flush timer variables
4. In the poll loop, check flush interval after each cycle
5. On SIGUSR1, do final flush instead of writing temp files
6. Remove all temp file writing code

Replace the section after `// Open /dev/tty for relay output` through the end of the function. The core addition is the flush logic. Here is the key section to add inside the poll loop, after the `continue` in the timeout branch:

```rust
// --- Periodic flush to DB ---
if last_flush.elapsed() >= flush_interval {
    let stdout_has_new = stdout_buf.len() > last_flush_stdout_len;
    let stderr_has_new = stderr_buf.len() > last_flush_stderr_len;

    if stdout_has_new || stderr_has_new {
        let stdout_clean = strip_ansi(&stdout_buf);
        let stderr_clean = strip_ansi(&stderr_buf);

        match on_detect {
            OnDetect::Redact => {
                let (stdout_redacted, _) = redact_with_custom_patterns(&stdout_clean, &custom_patterns);
                let (stderr_redacted, _) = redact_with_custom_patterns(&stderr_clean, &custom_patterns);
                let _ = db::update_command_output(
                    &db_conn, &config.command_id,
                    &stdout_redacted, &stderr_redacted,
                    stdout_truncated, stderr_truncated,
                );
            }
            OnDetect::Warn => {
                if !warn_logged {
                    let (_, stdout_labels) = redact_with_custom_patterns(&stdout_clean, &custom_patterns);
                    let (_, stderr_labels) = redact_with_custom_patterns(&stderr_clean, &custom_patterns);
                    if !stdout_labels.is_empty() || !stderr_labels.is_empty() {
                        eprintln!("[redtrail] WARN: secrets detected in output");
                        warn_logged = true;
                    }
                }
                let _ = db::update_command_output(
                    &db_conn, &config.command_id,
                    &stdout_clean, &stderr_clean,
                    stdout_truncated, stderr_truncated,
                );
            }
            OnDetect::Block => {
                let (_, labels) = redact_with_custom_patterns(&stdout_clean, &custom_patterns);
                let (_, stderr_labels) = redact_with_custom_patterns(&stderr_clean, &custom_patterns);
                if !labels.is_empty() || !stderr_labels.is_empty() {
                    let _ = db::delete_command(&db_conn, &config.command_id);
                    db_blocked = true;
                } else if !db_blocked {
                    let _ = db::update_command_output(
                        &db_conn, &config.command_id,
                        &stdout_clean, &stderr_clean,
                        stdout_truncated, stderr_truncated,
                    );
                }
            }
        }

        last_flush_stdout_len = stdout_buf.len();
        last_flush_stderr_len = stderr_buf.len();
    }
    last_flush = Instant::now();
}
```

- [ ] **Step 3: Remove dead code — temp file functions**

Remove from `src/core/tee.rs`:
- `TempFileHeader` struct
- `write_capture_file` function
- `read_capture_file` function

Check: no other file imports these. They were only used by `cmd/capture.rs` (which no longer reads temp files).

- [ ] **Step 4: Update `tests/tee_cli_test.rs`**

This file tests tee CLI args with `--session`. Update to `--command-id`. Read the file first to identify all calls that need updating.

- [ ] **Step 5: Verify compilation**

Run: `cargo build`
Expected: Compiles. May need to fix import paths.

- [ ] **Step 6: Run unit tests**

Run: `cargo test`
Expected: All tests pass including updated tee CLI tests.

- [ ] **Step 7: Commit**

```bash
git add src/core/tee.rs src/cmd/tee.rs src/cli.rs tests/tee_cli_test.rs
git commit -m "feat: tee streams output to DB every 1s with secret redaction"
```

---

## Task 4: Shell Hooks — Rewrite for start/finish flow

**Files:**
- Rewrite: `src/cmd/init.rs` (ZSH_HOOK and BASH_HOOK constants)
- Update: `tests/init_test.rs`

### Steps

- [ ] **Step 1: Rewrite ZSH_HOOK in `src/cmd/init.rs`**

Replace the entire `ZSH_HOOK` const. Key changes:
- `preexec` calls `redtrail capture start` synchronously, captures command ID
- `preexec` passes `--command-id` to tee instead of `--session`
- `precmd` calls `redtrail capture finish` backgrounded with `&!`
- `precmd` checks `__REDTRAIL_CMD_ID` instead of `__REDTRAIL_CMD`
- Remove temp file references

The full hook should match the spec's pseudocode in section "1. Shell Hooks", adapted from the current hook structure (blacklist check, alias resolution, escape hatch, PTY setup all preserved).

- [ ] **Step 2: Rewrite BASH_HOOK in `src/cmd/init.rs`**

Same changes as zsh but:
- `capture finish` uses `& disown` instead of `&!`
- DEBUG trap preserves `$?` with `local saved_exit=$?` at entry and `return $saved_exit` at exit
- `capture start` in DEBUG trap, `capture finish` in PROMPT_COMMAND

- [ ] **Step 3: Update `tests/init_test.rs`**

Update assertions to match new hook content:
- Check for `capture start` instead of `capture`
- Check for `capture finish` instead of `capture`
- Check for `--command-id` instead of `--session`
- Remove assertions about temp files (if any)
- Add assertion: zsh hook contains `&!` after `capture finish`
- Add assertion: bash hook contains `& disown` after `capture finish`
- Add assertion: bash hook preserves `$?` (`saved_exit`)

- [ ] **Step 4: Run unit tests**

Run: `cargo test`
Expected: All tests pass including updated init_test.rs.

- [ ] **Step 5: Commit**

```bash
git add src/cmd/init.rs tests/init_test.rs
git commit -m "feat: rewrite shell hooks for capture start/finish lifecycle"
```

---

## Task 5: Update existing live tests

**Files:**
- Update: `eval/tests/capture-basic.sh`
- Update: `eval/tests/capture-basic-bash.sh`
- Update: `eval/tests/capture-stdout.sh`
- Update: `eval/tests/capture-stdout-bash.sh`

### Steps

- [ ] **Step 1: Update `capture-basic.sh`**

The test sources `redtrail init zsh` which will automatically use the new hooks. The key verification change: add a check for the `status` column.

After the existing `grep -q "echo"` assertion, add:

```bash
# Verify command has status='finished'
STATUS_CHECK=$("$RT" query "SELECT status FROM commands WHERE command_binary = 'echo' LIMIT 1" --json 2>/dev/null)
echo "$STATUS_CHECK" | grep -q "finished" || {
  echo "FAIL: command status not 'finished'. Got: $STATUS_CHECK"
  exit 1
}
```

- [ ] **Step 2: Update `capture-basic-bash.sh`**

Same change as above but for the bash variant.

- [ ] **Step 3: Update `capture-stdout.sh`**

The existing query checks `stdout IS NOT NULL`. This should still work since tee now writes stdout directly to the DB. Add a status check:

```bash
STATUS_CHECK=$("$RT" query "SELECT status FROM commands WHERE command_binary = 'echo' AND stdout IS NOT NULL LIMIT 1" --json 2>/dev/null)
echo "$STATUS_CHECK" | grep -q "finished" || {
  echo "FAIL: stdout command not finished"
  exit 1
}
```

- [ ] **Step 4: Update `capture-stdout-bash.sh`**

Same change for bash variant.

- [ ] **Step 5: Build Docker image and run updated live tests**

Run: `make test-live` (or specific tests with `./scripts/live-test.sh capture-basic.sh`)
Expected: All four updated tests pass.

- [ ] **Step 6: Run the full existing live test suite**

Run: `./scripts/live-test.sh --fast`
Expected: All existing tests pass. Note any failures — they may need the same `status` column adjustments.

- [ ] **Step 7: Commit**

```bash
git add eval/tests/capture-basic.sh eval/tests/capture-basic-bash.sh eval/tests/capture-stdout.sh eval/tests/capture-stdout-bash.sh
git commit -m "test: update existing live tests for capture start/finish flow"
```

---

## Task 6: New live tests — streaming, redaction, rapid commands

**Files:**
- Create: `eval/tests/capture-streaming.sh`
- Create: `eval/tests/capture-streaming-redact.sh`
- Create: `eval/tests/capture-rapid-commands.sh`

### Steps

- [ ] **Step 1: Create `eval/tests/capture-streaming.sh`**

```bash
#!/usr/bin/env bash
# Live test: verify DB has partial stdout while a command is still running
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

# The command:
# 1. Runs a background loop that emits lines with sleep
# 2. Waits 3s, then queries DB mid-execution
# 3. Waits for loop, then exits
cat >"$TMPDIR/commands.txt" <<'CMDS'
{ for i in 1 2 3 4 5; do echo "stream-line-$i"; sleep 1; done; } &
BGPID=$!
sleep 3
/usr/local/bin/redtrail query "SELECT status, stdout FROM commands WHERE command_raw LIKE '%stream-line-%' LIMIT 1" --json > /tmp/rt-mid-check.json 2>/dev/null
wait $BGPID 2>/dev/null
sleep 1
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Check mid-execution snapshot: should have partial stdout
if [ -f /tmp/rt-mid-check.json ]; then
  # Verify we got at least some output mid-execution
  grep -q "stream-line-" /tmp/rt-mid-check.json || {
    echo "FAIL: no partial stdout found mid-execution. Got: $(cat /tmp/rt-mid-check.json)"
    exit 1
  }
else
  echo "FAIL: mid-execution check file not created"
  exit 1
fi

# Check final state: status should be finished, all lines present
FINAL=$("$RT" query "SELECT status, stdout FROM commands WHERE command_raw LIKE '%stream-line-%' LIMIT 1" --json 2>/dev/null)
echo "$FINAL" | grep -q "finished" || {
  echo "FAIL: final status not 'finished'. Got: $FINAL"
  exit 1
}

echo "PASS"
```

- [ ] **Step 2: Create `eval/tests/capture-streaming-redact.sh`**

```bash
#!/usr/bin/env bash
# Live test: verify secrets are redacted in DB even during streaming
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

# Command outputs an AWS key pattern after a delay
cat >"$TMPDIR/commands.txt" <<'CMDS'
{ sleep 1; echo "key=AKIAIOSFODNN7EXAMPLE"; sleep 2; } &
BGPID=$!
sleep 3
/usr/local/bin/redtrail query "SELECT stdout FROM commands WHERE stdout IS NOT NULL ORDER BY timestamp_start DESC LIMIT 1" --json > /tmp/rt-redact-check.json 2>/dev/null
wait $BGPID 2>/dev/null
sleep 1
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# The raw key should NOT appear in the DB — it should be redacted
FINAL=$("$RT" query "SELECT stdout FROM commands WHERE stdout LIKE '%AKIA%' OR stdout LIKE '%REDACTED%' ORDER BY timestamp_start DESC LIMIT 1" --json 2>/dev/null)

echo "$FINAL" | grep -q "AKIAIOSFODNN7EXAMPLE" && {
  echo "FAIL: raw AWS key found in DB (should be redacted)"
  exit 1
}

echo "$FINAL" | grep -q "REDACTED" || {
  echo "FAIL: no redaction marker found. Got: $FINAL"
  exit 1
}

echo "PASS"
```

- [ ] **Step 3: Create `eval/tests/capture-rapid-commands.sh`**

```bash
#!/usr/bin/env bash
# Live test: rapid sequential commands all captured correctly
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

cat >"$TMPDIR/commands.txt" <<'CMDS'
echo rapid-1
echo rapid-2
echo rapid-3
echo rapid-4
echo rapid-5
echo rapid-6
echo rapid-7
echo rapid-8
echo rapid-9
echo rapid-10
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# All 10 echo commands should be captured
COUNT=$("$RT" query "SELECT COUNT(*) as cnt FROM commands WHERE command_raw LIKE 'echo rapid-%'" --json 2>/dev/null)
echo "$COUNT" | grep -q '"cnt":10' || echo "$COUNT" | grep -q '"cnt": 10' || {
  echo "FAIL: expected 10 rapid commands captured. Got: $COUNT"
  exit 1
}

# All should be finished
RUNNING=$("$RT" query "SELECT COUNT(*) as cnt FROM commands WHERE command_raw LIKE 'echo rapid-%' AND status != 'finished'" --json 2>/dev/null)
echo "$RUNNING" | grep -q '"cnt":0' || echo "$RUNNING" | grep -q '"cnt": 0' || {
  echo "FAIL: some rapid commands still running. Got: $RUNNING"
  exit 1
}

echo "PASS"
```

- [ ] **Step 4: Create `eval/tests/capture-streaming-block.sh`**

```bash
#!/usr/bin/env bash
# Live test: on_detect=block deletes command row when secret appears in streaming output
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

mkdir -p "$TMPDIR/.config/redtrail"
cat >"$TMPDIR/.config/redtrail/config.yaml" <<'CONF'
secrets:
  on_detect: block
CONF

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

cat >"$TMPDIR/commands.txt" <<'CMDS'
{ sleep 1; echo "key=AKIAIOSFODNN7EXAMPLE"; sleep 2; } &
BGPID=$!
wait $BGPID 2>/dev/null
sleep 2
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# The command row should have been deleted by tee (on_detect=block)
COUNT=$("$RT" query "SELECT COUNT(*) as cnt FROM commands WHERE command_raw LIKE '%AKIA%'" --json 2>/dev/null)
echo "$COUNT" | grep -q '"cnt":0' || echo "$COUNT" | grep -q '"cnt": 0' || {
  echo "FAIL: command with secret should have been deleted. Got: $COUNT"
  exit 1
}

echo "PASS"
```

- [ ] **Step 5: Create `eval/tests/capture-orphan-cleanup.sh`**

```bash
#!/usr/bin/env bash
# Live test: stale 'running' commands from >24h ago are cleaned up
set -euo pipefail

RT="/usr/local/bin/redtrail"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"
export REDTRAIL_DB="$TMPDIR/test.db"

cat >"$TMPDIR/.zshrc" <<'EOF'
eval "$(/usr/local/bin/redtrail init zsh)"
setopt NO_HUP
setopt NO_CHECK_JOBS
EOF

# Pre-populate DB with a stale running command
cat >"$TMPDIR/commands.txt" <<'CMDS'
echo "trigger-cleanup"
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands.txt" >/dev/null 2>&1 || true

# Get the session ID used
SESSION_ID=$("$RT" query "SELECT session_id FROM commands LIMIT 1" --json 2>/dev/null | grep -o '"session_id":"[^"]*"' | head -1 | cut -d'"' -f4)

# Manually insert a stale running command with same session
OLD_TS=$(( $(date +%s) - 100000 ))
"$RT" query "INSERT INTO commands (id, session_id, timestamp_start, command_raw, source, status) VALUES ('stale-orphan', '$SESSION_ID', $OLD_TS, 'stale command', 'human', 'running')" 2>/dev/null || true

# Run another command to trigger orphan cleanup (happens in capture start)
cat >"$TMPDIR/commands2.txt" <<'CMDS'
echo "second-trigger"
exit
CMDS

HOME="$TMPDIR" script -q -c "zsh -i" /dev/null <"$TMPDIR/commands2.txt" >/dev/null 2>&1 || true

# Check the stale command was marked orphaned
STATUS=$("$RT" query "SELECT status FROM commands WHERE id = 'stale-orphan'" --json 2>/dev/null)
echo "$STATUS" | grep -q "orphaned" || {
  echo "FAIL: stale running command not marked orphaned. Got: $STATUS"
  exit 1
}

echo "PASS"
```

- [ ] **Step 6: Make all test scripts executable**

```bash
chmod +x eval/tests/capture-streaming.sh eval/tests/capture-streaming-redact.sh eval/tests/capture-rapid-commands.sh eval/tests/capture-streaming-block.sh eval/tests/capture-orphan-cleanup.sh
```

- [ ] **Step 7: Build and run new live tests**

Run each individually:
```bash
./scripts/live-test.sh capture-streaming
./scripts/live-test.sh capture-streaming-redact
./scripts/live-test.sh capture-rapid-commands
./scripts/live-test.sh capture-streaming-block
./scripts/live-test.sh capture-orphan-cleanup
```
Expected: All PASS.

- [ ] **Step 8: Run full live test suite**

Run: `./scripts/live-test.sh --fast`
Expected: All tests pass (existing + new).

- [ ] **Step 9: Commit**

```bash
git add eval/tests/capture-streaming.sh eval/tests/capture-streaming-redact.sh eval/tests/capture-rapid-commands.sh eval/tests/capture-streaming-block.sh eval/tests/capture-orphan-cleanup.sh
git commit -m "test: add streaming capture live tests — streaming, redaction, block mode, orphan cleanup, rapid commands"
```

---

## Task 7: Cleanup — remove dead code, final verification

**Files:**
- Modify: `src/core/tee.rs` (verify temp file code removed)
- Modify: `src/cmd/init.rs` (verify no temp file references)

### Steps

- [ ] **Step 1: Grep for dead code references**

Run: `grep -rn "read_capture_file\|write_capture_file\|TempFileHeader\|rt-out-\|rt-err-" src/`
Expected: No matches (all temp file code removed in Task 3).

If any remain, remove them.

- [ ] **Step 2: Grep for old capture CLI references in hooks**

Run: `grep -n "stdout-file\|stderr-file\|stdout_file\|stderr_file" src/`
Expected: No matches in hook code or capture code.

- [ ] **Step 3: Run full test suite — unit + live**

Run: `cargo test && ./scripts/live-test.sh --fast`
Expected: Everything passes.

- [ ] **Step 4: Final commit (only if dead code was found and removed)**

```bash
git add src/core/tee.rs src/cmd/init.rs src/cmd/capture.rs
git commit -m "chore: remove dead temp file code from capture layer"
```
