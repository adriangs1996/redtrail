# Stdout/Stderr Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Capture stdout/stderr from shell commands via a PTY-aware `redtrail tee` binary, storing output in the DB alongside command metadata.

**Architecture:** `redtrail tee` is a single Rust binary that allocates two PTY pairs (stdout + stderr), exposes slave paths via a FIFO, relays PTY master output to `/dev/tty`, and writes captured output to temp files on EOF. Shell hooks redirect stdout/stderr to the PTY slaves. `redtrail capture` reads temp files and inserts the complete command record.

**Tech Stack:** Rust, `nix` crate (PTY + ioctl), `strip-ansi-escapes` crate, rusqlite

**Spec:** `docs/superpowers/specs/2026-03-27-stdout-capture-design.md`

**Key design decisions:**
- **Timestamps stay as seconds in the DB.** `redtrail tee` uses Rust's `SystemTime` for precision, but converts to seconds before writing temp file headers. This avoids migrating every time-based query (`--today`, `--since`, `forget --last`, session timestamps) to nanoseconds. We can do that migration separately later.
- **Single redaction point.** `redtrail tee` strips ANSI only. `redtrail capture` handles secret redaction via `insert_command_redacted` (existing code). No double redaction.
- **Test isolation.** Tests use random PIDs and `tempfile`-managed directories to avoid `/tmp` collisions in parallel test runs.

---

## File Map

### New files

| File | Responsibility |
|------|---------------|
| `src/core/tee.rs` | PTY allocation, relay loop, ANSI stripping, capture buffer, temp file writing, signal handling — the core `redtrail tee` logic |
| `src/cmd/tee.rs` | CLI entry point for `redtrail tee` subcommand — parses args, calls `core::tee::run()` |
| `tests/tee_test.rs` | Unit/integration tests for PTY relay, ANSI stripping, buffer truncation, temp file format |
| `tests/tee_cli_test.rs` | CLI integration tests for the `redtrail tee` subcommand |
| `tests/capture_stdout_test.rs` | Tests for `redtrail capture --stdout-file --stderr-file` flow |

### Modified files

| File | Changes |
|------|---------|
| `Cargo.toml` | Add `nix` and `strip-ansi-escapes` dependencies |
| `src/core/mod.rs` | Add `pub mod tee;` |
| `src/cmd/mod.rs` | Add `pub mod tee;` |
| `src/cli.rs` | Add `Tee` subcommand variant, wire to `cmd::tee::run()` |
| `src/cmd/capture.rs` | Add `--stdout-file`/`--stderr-file` support, parse temp file headers |
| `src/core/db.rs` | Add `stderr_truncated` to schema, `NewCommand`, and `CommandRow` |
| `src/cmd/init.rs` | Update zsh and bash hook scripts with FIFO/PTY setup |
| `src/error.rs` | Add `Pty(String)` variant for PTY-specific errors |

---

## Task 1: Add dependencies and error variant

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/error.rs`

- [ ] **Step 1: Add `nix` and `strip-ansi-escapes` to Cargo.toml**

Add to `[dependencies]`:
```toml
nix = { version = "0.29", features = ["term", "pty", "signal", "poll", "fs"] }
strip-ansi-escapes = "0.2"
```

- [ ] **Step 2: Add `Pty` error variant**

In `src/error.rs`, add `Pty(String)` to the `Error` enum and update the `Display` impl:
```rust
Pty(String),
```
Display arm:
```rust
Error::Pty(e) => write!(f, "pty error: {e}"),
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: Success, no errors.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/error.rs
git commit -m "feat: add nix and strip-ansi-escapes deps, Pty error variant"
```

---

## Task 2: Add `stderr_truncated` to DB schema

**Files:**
- Modify: `src/core/db.rs`
- Test: `tests/capture_test.rs`

- [ ] **Step 1: Write test for stderr_truncated column**

Add to `tests/capture_test.rs`:
```rust
#[test]
fn insert_command_stores_stderr_truncated() {
    let conn = setup();

    db::insert_command(
        &conn,
        &db::NewCommand {
            session_id: "s1",
            command_raw: "make",
            stderr: Some("error output"),
            stderr_truncated: true,
            timestamp_start: 1000,
            source: "human",
            ..Default::default()
        },
    )
    .unwrap();

    let cmds = db::get_commands(&conn, &db::CommandFilter::default()).unwrap();
    assert!(cmds[0].stderr_truncated);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test capture_test insert_command_stores_stderr_truncated`
Expected: Compile error — `stderr_truncated` field doesn't exist on `NewCommand` or `CommandRow`.

- [ ] **Step 3: Add `stderr_truncated` to schema, `NewCommand`, and `CommandRow`**

In `src/core/db.rs`:

1. In the `SCHEMA` string, add after `stdout_truncated BOOLEAN DEFAULT 0,`:
```sql
    stderr_truncated BOOLEAN DEFAULT 0,
```

2. Add field to `NewCommand`:
```rust
pub stderr_truncated: bool,
```

3. Add field to `CommandRow`:
```rust
pub stderr_truncated: bool,
```

4. Update `insert_command` SQL to include `stderr_truncated` — add `?22` parameter and include `cmd.stderr_truncated` in the params.

5. Update `get_commands` SELECT to include `stderr_truncated` — add it after `redacted` (column index 15), and update the `CommandRow` mapping.

6. Update `search_commands` SELECT and `CommandRow` mapping to include `stderr_truncated` — this function also builds `CommandRow` from a query and must be kept in sync.

7. Update `insert_command_redacted` to propagate `stderr_truncated`:
```rust
stderr_truncated: cmd.stderr_truncated,
```
(already handled by the `..* cmd` spread, since it's in `NewCommand`)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test capture_test insert_command_stores_stderr_truncated`
Expected: PASS

- [ ] **Step 5: Run all tests to verify no regressions**

Run: `cargo test`
Expected: All 313+ tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/core/db.rs tests/capture_test.rs
git commit -m "feat: add stderr_truncated column to commands schema"
```

---

## Task 3: Implement temp file header parsing

**Files:**
- Create: `src/core/tee.rs`
- Modify: `src/core/mod.rs`
- Test: `tests/tee_test.rs`

This task builds the temp file format parser used by both `redtrail tee` (writing) and `redtrail capture` (reading). We build and test it in isolation before the PTY code.

- [ ] **Step 1: Register the module**

In `src/core/mod.rs`, add:
```rust
pub mod tee;
```

- [ ] **Step 2: Write tests for temp file header parsing and writing**

Create `tests/tee_test.rs`:
```rust
use redtrail::core::tee::{TempFileHeader, write_capture_file, read_capture_file};
use std::io::Write;

#[test]
fn write_and_read_capture_file_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rt-out-12345");

    let header = TempFileHeader {
        ts_start: 1711555200,
        ts_end: 1711555203,
        truncated: false,
    };
    write_capture_file(&path, &header, "hello world\n").unwrap();

    let (h, content) = read_capture_file(&path).unwrap();
    assert_eq!(h.ts_start, 1711555200i64);
    assert_eq!(h.ts_end, 1711555203i64);
    assert!(!h.truncated);
    assert_eq!(content, "hello world\n");
}

#[test]
fn read_capture_file_with_truncated_flag() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rt-out-12345");

    let header = TempFileHeader {
        ts_start: 1000,
        ts_end: 2000,
        truncated: true,
    };
    write_capture_file(&path, &header, "partial output").unwrap();

    let (h, content) = read_capture_file(&path).unwrap();
    assert!(h.truncated);
    assert_eq!(content, "partial output");
}

#[test]
fn read_capture_file_returns_none_for_missing_file() {
    let result = read_capture_file(std::path::Path::new("/tmp/nonexistent-rt-file"));
    assert!(result.is_none());
}

#[test]
fn write_capture_file_sets_permissions_to_600() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rt-out-perms");

    let header = TempFileHeader {
        ts_start: 1000,
        ts_end: 2000,
        truncated: false,
    };
    write_capture_file(&path, &header, "secret output").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --test tee_test`
Expected: Compile error — module/types don't exist yet.

- [ ] **Step 4: Implement temp file header, write, and read**

Create `src/core/tee.rs`:
```rust
use crate::error::Error;
use std::path::Path;

/// Header metadata written to stdout/stderr capture temp files.
pub struct TempFileHeader {
    pub ts_start: i64,
    pub ts_end: i64,
    pub truncated: bool,
}

/// Write a capture temp file with header and content. File is created with mode 0600.
pub fn write_capture_file(
    path: &Path,
    header: &TempFileHeader,
    content: &str,
) -> Result<(), Error> {
    use std::io::Write;

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        write!(
            f,
            "ts_start:{}\nts_end:{}\ntruncated:{}\n\n{}",
            header.ts_start, header.ts_end, header.truncated, content
        )?;
    }

    #[cfg(not(unix))]
    {
        let mut f = std::fs::File::create(path)?;
        write!(
            f,
            "ts_start:{}\nts_end:{}\ntruncated:{}\n\n{}",
            header.ts_start, header.ts_end, header.truncated, content
        )?;
    }

    Ok(())
}

/// Read a capture temp file. Returns None if the file doesn't exist.
/// Returns (header, content) on success.
pub fn read_capture_file(path: &Path) -> Option<(TempFileHeader, String)> {
    let data = std::fs::read_to_string(path).ok()?;

    let mut ts_start: i64 = 0;
    let mut ts_end: i64 = 0;
    let mut truncated = false;

    // Find the blank line separating header from content
    let content_start = data.find("\n\n").map(|i| i + 2).unwrap_or(data.len());
    let header_section = &data[..content_start.saturating_sub(2).min(data.len())];

    for line in header_section.lines() {
        if let Some(val) = line.strip_prefix("ts_start:") {
            ts_start = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("ts_end:") {
            ts_end = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("truncated:") {
            truncated = val == "true";
        }
    }

    let content = if content_start < data.len() {
        data[content_start..].to_string()
    } else {
        String::new()
    };

    Some((
        TempFileHeader {
            ts_start,
            ts_end,
            truncated,
        },
        content,
    ))
}

/// Strip ANSI escape sequences from terminal output.
pub fn strip_ansi(input: &[u8]) -> String {
    let stripped = strip_ansi_escapes::strip(input);
    String::from_utf8_lossy(&stripped).to_string()
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test tee_test`
Expected: All 4 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add src/core/tee.rs src/core/mod.rs tests/tee_test.rs
git commit -m "feat: add temp file header format and ANSI stripping for tee capture"
```

---

## Task 4: Implement `--stdout-file`/`--stderr-file` on `redtrail capture`

**Files:**
- Modify: `src/cmd/capture.rs`
- Modify: `src/cli.rs`
- Create: `tests/capture_stdout_test.rs`

- [ ] **Step 1: Write CLI integration test for stdout-file**

Create `tests/capture_stdout_test.rs`:
```rust
use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

fn setup_db() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let _conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    dir
}

#[test]
fn capture_reads_stdout_from_file() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    // Write a temp capture file
    let stdout_file = dir.path().join("rt-out-test");
    redtrail::core::tee::write_capture_file(
        &stdout_file,
        &redtrail::core::tee::TempFileHeader {
            ts_start: 1000,
            ts_end: 2000,
            truncated: false,
        },
        "hello from stdout\n",
    )
    .unwrap();

    let output = redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "echo hello",
            "--exit-code", "0",
            "--shell", "zsh",
            "--hostname", "devbox",
            "--stdout-file",
            stdout_file.to_str().unwrap(),
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(
        &conn,
        &redtrail::core::db::CommandFilter::default(),
    )
    .unwrap();

    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].stdout.as_deref(), Some("hello from stdout\n"));
    assert!(!cmds[0].stdout_truncated);

    // Temp file should be deleted by capture
    assert!(!stdout_file.exists(), "capture should delete the temp file");
}

#[test]
fn capture_reads_stderr_from_file() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let stderr_file = dir.path().join("rt-err-test");
    redtrail::core::tee::write_capture_file(
        &stderr_file,
        &redtrail::core::tee::TempFileHeader {
            ts_start: 1000,
            ts_end: 2000,
            truncated: true,
        },
        "error output\n",
    )
    .unwrap();

    let output = redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "make build",
            "--exit-code", "1",
            "--shell", "zsh",
            "--hostname", "devbox",
            "--stderr-file",
            stderr_file.to_str().unwrap(),
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(
        &conn,
        &redtrail::core::db::CommandFilter::default(),
    )
    .unwrap();

    assert_eq!(cmds[0].stderr.as_deref(), Some("error output\n"));
    assert!(cmds[0].stderr_truncated);
    assert!(!stderr_file.exists());
}

#[test]
fn capture_uses_timestamps_from_temp_file_header() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let stdout_file = dir.path().join("rt-out-ts");
    redtrail::core::tee::write_capture_file(
        &stdout_file,
        &redtrail::core::tee::TempFileHeader {
            ts_start: 5000,
            ts_end: 7000,
            truncated: false,
        },
        "output",
    )
    .unwrap();

    redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "echo test",
            "--exit-code", "0",
            "--shell", "zsh",
            "--hostname", "devbox",
            "--stdout-file",
            stdout_file.to_str().unwrap(),
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(
        &conn,
        &redtrail::core::db::CommandFilter::default(),
    )
    .unwrap();

    assert_eq!(cmds[0].timestamp_start, 5000);
    assert_eq!(cmds[0].timestamp_end, Some(7000));
}

#[test]
fn capture_without_files_still_works() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let output = redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "ls",
            "--exit-code", "0",
            "--ts-start", "1000",
            "--shell", "zsh",
            "--hostname", "devbox",
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(
        &conn,
        &redtrail::core::db::CommandFilter::default(),
    )
    .unwrap();

    assert_eq!(cmds.len(), 1);
    assert!(cmds[0].stdout.is_none());
    assert!(cmds[0].stderr.is_none());
    assert_eq!(cmds[0].timestamp_start, 1000);
}

#[test]
fn capture_redacts_secrets_in_stdout_file() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");

    let stdout_file = dir.path().join("rt-out-secret");
    redtrail::core::tee::write_capture_file(
        &stdout_file,
        &redtrail::core::tee::TempFileHeader {
            ts_start: 1000,
            ts_end: 2000,
            truncated: false,
        },
        "aws_access_key_id=AKIAIOSFODNN7EXAMPLE\n",
    )
    .unwrap();

    redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "cat credentials",
            "--exit-code", "0",
            "--shell", "zsh",
            "--hostname", "devbox",
            "--stdout-file",
            stdout_file.to_str().unwrap(),
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(
        &conn,
        &redtrail::core::db::CommandFilter::default(),
    )
    .unwrap();

    let stdout = cmds[0].stdout.as_ref().unwrap();
    assert!(!stdout.contains("AKIAIOSFODNN7EXAMPLE"), "secret should be redacted in stdout");
    assert!(cmds[0].redacted);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test capture_stdout_test`
Expected: Compile errors — `--stdout-file` flag doesn't exist.

- [ ] **Step 3: Add `--stdout-file` and `--stderr-file` flags to CLI**

In `src/cli.rs`, add to the `Capture` variant:
```rust
#[arg(long)]
stdout_file: Option<String>,
#[arg(long)]
stderr_file: Option<String>,
```

Update the `Commands::Capture` match arm to pass them:
```rust
Commands::Capture { session_id, command, cwd, exit_code, ts_start, ts_end, shell, hostname, stdout_file, stderr_file } => {
    let conn = open_db()?;
    cmd::capture::run(&conn, &cmd::capture::CaptureArgs {
        session_id: &session_id,
        command: &command,
        cwd: cwd.as_deref(),
        exit_code,
        ts_start,
        ts_end,
        shell: shell.as_deref(),
        hostname: hostname.as_deref(),
        stdout_file: stdout_file.as_deref(),
        stderr_file: stderr_file.as_deref(),
    })
}
```

Note: `ts_start` and `ts_end` remain as optional CLI args for backward compatibility (existing hooks without tee), but when stdout-file/stderr-file are provided, their headers take precedence.

Make `ts_start` optional in the CLI:
```rust
#[arg(long)]
ts_start: Option<i64>,
#[arg(long)]
ts_end: Option<i64>,
```

- [ ] **Step 4: Update `CaptureArgs` and `cmd::capture::run`**

In `src/cmd/capture.rs`, update `CaptureArgs`:
```rust
pub struct CaptureArgs<'a> {
    pub session_id: &'a str,
    pub command: &'a str,
    pub cwd: Option<&'a str>,
    pub exit_code: Option<i32>,
    pub ts_start: Option<i64>,
    pub ts_end: Option<i64>,
    pub shell: Option<&'a str>,
    pub hostname: Option<&'a str>,
    pub stdout_file: Option<&'a str>,
    pub stderr_file: Option<&'a str>,
}
```

Update `run` to read temp files:
```rust
pub fn run(conn: &Connection, args: &CaptureArgs) -> Result<(), Error> {
    use crate::core::tee;

    let parsed = capture::parse_command(args.command);

    let blacklist = capture::default_blacklist();
    if capture::is_blacklisted(&parsed.binary, &blacklist) {
        return Ok(());
    }

    // Read stdout/stderr from temp files if provided
    let stdout_capture = args.stdout_file.and_then(|p| tee::read_capture_file(std::path::Path::new(p)));
    let stderr_capture = args.stderr_file.and_then(|p| tee::read_capture_file(std::path::Path::new(p)));

    // Use timestamps from temp file headers if available, else fall back to CLI args, else generate now
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let ts_start = stdout_capture.as_ref().map(|(h, _)| h.ts_start as i64)
        .or(stderr_capture.as_ref().map(|(h, _)| h.ts_start as i64))
        .or(args.ts_start)
        .unwrap_or(now_secs);

    let ts_end = stdout_capture.as_ref().map(|(h, _)| h.ts_end as i64)
        .or(stderr_capture.as_ref().map(|(h, _)| h.ts_end as i64))
        .or(args.ts_end);

    let stdout_content = stdout_capture.as_ref().map(|(_, c)| c.as_str());
    let stderr_content = stderr_capture.as_ref().map(|(_, c)| c.as_str());
    let stdout_truncated = stdout_capture.as_ref().is_some_and(|(h, _)| h.truncated);
    let stderr_truncated = stderr_capture.as_ref().is_some_and(|(h, _)| h.truncated);

    let git = args.cwd.map(capture::git_context);
    let git_repo = git.as_ref().and_then(|g| g.repo.as_deref());
    let git_branch = git.as_ref().and_then(|g| g.branch.as_deref());

    let env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let env_snap = capture::env_snapshot(&env);
    let source = capture::detect_source(&env, None);

    let args_json = serde_json::to_string(&parsed.args).unwrap_or_default();
    let flags_json = serde_json::to_string(&parsed.flags).unwrap_or_default();

    db::insert_command_redacted(
        conn,
        &db::NewCommand {
            session_id: args.session_id,
            command_raw: args.command,
            command_binary: if parsed.binary.is_empty() { None } else { Some(&parsed.binary) },
            command_subcommand: parsed.subcommand.as_deref(),
            command_args: Some(&args_json),
            command_flags: Some(&flags_json),
            cwd: args.cwd,
            git_repo,
            git_branch,
            exit_code: args.exit_code,
            stdout: stdout_content,
            stderr: stderr_content,
            stdout_truncated,
            stderr_truncated,
            timestamp_start: ts_start,
            timestamp_end: ts_end,
            shell: args.shell,
            hostname: args.hostname,
            env_snapshot: Some(&env_snap),
            source,
            ..Default::default()
        },
    )?;

    // Clean up temp files
    if let Some(path) = args.stdout_file {
        let _ = std::fs::remove_file(path);
    }
    if let Some(path) = args.stderr_file {
        let _ = std::fs::remove_file(path);
    }

    Ok(())
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test capture_stdout_test`
Expected: All 5 tests PASS.

- [ ] **Step 6: Run all tests to verify no regressions**

Run: `cargo test`
Expected: All tests pass (including the existing `capture_cli_test` tests which use `--ts-start` — backward compatible).

- [ ] **Step 7: Commit**

```bash
git add src/cli.rs src/cmd/capture.rs tests/capture_stdout_test.rs
git commit -m "feat: add --stdout-file/--stderr-file to redtrail capture"
```

---

## Task 5: Implement the `redtrail tee` core — PTY allocation and relay

**Files:**
- Modify: `src/core/tee.rs`
- Test: `tests/tee_test.rs`

This is the largest task. It implements the PTY allocation, FIFO handshake, relay loop, and temp file output.

- [ ] **Step 1: Write test for PTY allocation and relay**

Add to `tests/tee_test.rs`:
```rust
#[test]
fn pty_relay_captures_output() {
    use std::io::{Read, Write};
    use redtrail::core::tee::{allocate_pty_pair, TempFileHeader};

    // Allocate a PTY pair
    let pty = allocate_pty_pair().expect("PTY allocation should succeed");

    // Write to the slave (simulating a command's stdout)
    let mut slave = std::fs::OpenOptions::new()
        .write(true)
        .open(&pty.slave_path)
        .unwrap();
    slave.write_all(b"hello from pty\n").unwrap();
    drop(slave); // close slave → master gets EOF

    // Read from the master (what redtrail tee does)
    let mut buf = vec![0u8; 1024];
    let n = nix::unistd::read(pty.master_fd.as_raw_fd(), &mut buf).unwrap();

    assert!(n > 0);
    let output = String::from_utf8_lossy(&buf[..n]);
    assert!(output.contains("hello from pty"), "got: {output}");
}

#[test]
fn strip_ansi_removes_color_codes() {
    use redtrail::core::tee::strip_ansi;

    let colored = "\x1b[32mgreen text\x1b[0m and normal";
    let stripped = strip_ansi(colored.as_bytes());
    assert_eq!(stripped, "green text and normal");
}

#[test]
fn strip_ansi_handles_osc_sequences() {
    use redtrail::core::tee::strip_ansi;

    // OSC title-setting sequence
    let input = "\x1b]0;My Terminal Title\x07some output";
    let stripped = strip_ansi(input.as_bytes());
    assert_eq!(stripped, "some output");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test tee_test pty_relay`
Expected: Compile error — `allocate_pty_pair` doesn't exist.

- [ ] **Step 3: Implement `allocate_pty_pair`**

Add to `src/core/tee.rs`:
```rust
use std::os::fd::{AsRawFd, OwnedFd};

/// A PTY master/slave pair.
pub struct PtyPair {
    pub master_fd: OwnedFd,
    pub slave_path: String,
}

/// Allocate a PTY pair. Returns the master fd and the slave device path.
pub fn allocate_pty_pair() -> Result<PtyPair, Error> {
    use nix::pty::{posix_openpt, grantpt, unlockpt, ptsname_r};
    use nix::fcntl::OFlag;

    let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY)
        .map_err(|e| Error::Pty(format!("posix_openpt: {e}")))?;

    grantpt(&master).map_err(|e| Error::Pty(format!("grantpt: {e}")))?;
    unlockpt(&master).map_err(|e| Error::Pty(format!("unlockpt: {e}")))?;

    let slave_path = ptsname_r(&master)
        .map_err(|e| Error::Pty(format!("ptsname_r: {e}")))?;

    Ok(PtyPair {
        master_fd: master,
        slave_path,
    })
}

/// Set the window size on a PTY slave fd from the current /dev/tty dimensions.
pub fn init_pty_winsize(slave_path: &str) -> Result<(), Error> {
    use nix::libc;
    use std::fs::OpenOptions;

    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|e| Error::Pty(format!("open /dev/tty: {e}")))?;

    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::ioctl(tty.as_raw_fd(), libc::TIOCGWINSZ, &mut ws) };
    if ret != 0 {
        return Err(Error::Pty("TIOCGWINSZ failed".into()));
    }

    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(nix::libc::O_NOCTTY)
        .open(slave_path)
        .map_err(|e| Error::Pty(format!("open slave: {e}")))?;

    let ret = unsafe { libc::ioctl(slave.as_raw_fd(), libc::TIOCSWINSZ, &ws) };
    if ret != 0 {
        return Err(Error::Pty("TIOCSWINSZ failed".into()));
    }

    Ok(())
}
```

- [ ] **Step 4: Run PTY tests**

Run: `cargo test --test tee_test`
Expected: All tests pass (PTY allocation, ANSI stripping, temp file roundtrip).

- [ ] **Step 5: Commit**

```bash
git add src/core/tee.rs tests/tee_test.rs
git commit -m "feat: add PTY allocation and window size init for tee capture"
```

---

## Task 6: Implement the `redtrail tee` relay loop and CLI entry point

**Files:**
- Modify: `src/core/tee.rs`
- Create: `src/cmd/tee.rs`
- Modify: `src/cmd/mod.rs`
- Modify: `src/cli.rs`
- Create: `tests/tee_cli_test.rs`

- [ ] **Step 1: Write CLI integration test**

Create `tests/tee_cli_test.rs`:
```rust
use std::io::{Read, Write};
use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

#[test]
fn tee_creates_pty_and_writes_paths_to_fifo() {
    let dir = tempfile::tempdir().unwrap();
    let fifo_path = dir.path().join("ctl-fifo");
    let shell_pid = format!("test-{}", std::process::id());

    // Create FIFO
    nix::unistd::mkfifo(&fifo_path, nix::sys::stat::Mode::from_bits_truncate(0o600))
        .expect("mkfifo should succeed");

    // Start tee in background
    let mut child = redtrail_bin()
        .args([
            "tee",
            "--session", "test-sess",
            "--shell-pid", &shell_pid,
            "--ctl-fifo", fifo_path.to_str().unwrap(),
        ])
        .spawn()
        .expect("failed to start tee");

    // Read PTY paths from FIFO (should unblock within 1s)
    let fifo_content = std::fs::read_to_string(&fifo_path).unwrap();
    let paths: Vec<&str> = fifo_content.trim().split_whitespace().collect();

    assert_eq!(paths.len(), 2, "should get two PTY slave paths, got: {fifo_content}");
    assert!(std::path::Path::new(paths[0]).exists(), "stdout PTY slave should exist");
    assert!(std::path::Path::new(paths[1]).exists(), "stderr PTY slave should exist");

    // Write to the stdout PTY slave, then close it to trigger EOF
    {
        let mut slave = std::fs::OpenOptions::new()
            .write(true)
            .open(paths[0])
            .unwrap();
        slave.write_all(b"captured output\n").unwrap();
    }
    // Close stderr slave too
    {
        let _ = std::fs::OpenOptions::new().write(true).open(paths[1]);
    }

    // Wait for tee to exit
    let status = child.wait().expect("wait failed");
    assert!(status.success(), "tee should exit cleanly");

    // Check temp files were created
    let out_file = format!("/tmp/rt-out-{shell_pid}");
    assert!(std::path::Path::new(&out_file).exists(), "stdout temp file should exist");

    let (header, content) = redtrail::core::tee::read_capture_file(std::path::Path::new(&out_file)).unwrap();
    assert!(content.contains("captured output"), "content: {content}");
    assert!(!header.truncated);
    assert!(header.ts_start > 0);
    assert!(header.ts_end >= header.ts_start);

    // Cleanup
    let _ = std::fs::remove_file(&out_file);
    let _ = std::fs::remove_file(format!("/tmp/rt-err-{}", shell_pid));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test tee_cli_test`
Expected: Compile error — `tee` subcommand doesn't exist.

- [ ] **Step 3: Implement the relay loop in `core::tee`**

Add to `src/core/tee.rs`:
```rust
/// Configuration for the tee relay loop.
pub struct TeeConfig {
    pub session_id: String,
    pub shell_pid: String,
    pub ctl_fifo: String,
    pub max_bytes: usize,
}

/// Run the tee relay: allocate PTYs, write paths to FIFO, relay output, write temp files on EOF.
pub fn run_tee(config: &TeeConfig) -> Result<(), Error> {
    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
    use std::io::Write;

    // Allocate two PTY pairs
    let stdout_pty = allocate_pty_pair()?;
    let stderr_pty = allocate_pty_pair()?;

    // Initialize window size (best-effort — may fail in non-TTY environments)
    let _ = init_pty_winsize(&stdout_pty.slave_path);
    let _ = init_pty_winsize(&stderr_pty.slave_path);

    // Record start time (seconds — matches DB schema)
    let ts_start = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Write slave paths to FIFO
    {
        let mut fifo = std::fs::OpenOptions::new()
            .write(true)
            .open(&config.ctl_fifo)?;
        writeln!(fifo, "{} {}", stdout_pty.slave_path, stderr_pty.slave_path)?;
    }

    // Open /dev/tty for relay output
    let mut tty = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/tty")?;

    // Capture buffers
    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let mut stdout_truncated = false;
    let mut stderr_truncated = false;

    // Relay loop — poll both masters
    let stdout_fd = stdout_pty.master_fd.as_raw_fd();
    let stderr_fd = stderr_pty.master_fd.as_raw_fd();
    let mut stdout_eof = false;
    let mut stderr_eof = false;
    let mut read_buf = [0u8; 4096];

    // Set up inactivity timeout (5 minutes)
    let inactivity_timeout = std::time::Duration::from_secs(300);
    let mut last_activity = std::time::Instant::now();

    while !stdout_eof || !stderr_eof {
        let mut pollfds = Vec::new();
        if !stdout_eof {
            pollfds.push(PollFd::new(
                unsafe { std::os::fd::BorrowedFd::borrow_raw(stdout_fd) },
                PollFlags::POLLIN,
            ));
        }
        if !stderr_eof {
            pollfds.push(PollFd::new(
                unsafe { std::os::fd::BorrowedFd::borrow_raw(stderr_fd) },
                PollFlags::POLLIN,
            ));
        }

        let poll_result = poll(&mut pollfds, PollTimeout::from(1000u16));
        match poll_result {
            Ok(0) => {
                // Timeout — check inactivity
                if last_activity.elapsed() > inactivity_timeout {
                    break;
                }
                continue;
            }
            Ok(_) => {}
            Err(nix::errno::Errno::EINTR) => continue,
            Err(_) => break,
        }

        let mut pf_idx = 0;

        if !stdout_eof {
            let revents = pollfds[pf_idx].revents().unwrap_or(PollFlags::empty());
            if revents.contains(PollFlags::POLLIN) {
                match nix::unistd::read(stdout_fd, &mut read_buf) {
                    Ok(0) => stdout_eof = true,
                    Ok(n) => {
                        last_activity = std::time::Instant::now();
                        let _ = tty.write_all(&read_buf[..n]);
                        if stdout_buf.len() < config.max_bytes {
                            let remaining = config.max_bytes - stdout_buf.len();
                            let take = n.min(remaining);
                            stdout_buf.extend_from_slice(&read_buf[..take]);
                            if n > remaining {
                                stdout_truncated = true;
                            }
                        }
                    }
                    Err(nix::errno::Errno::EIO) => stdout_eof = true, // PTY slave closed
                    Err(_) => stdout_eof = true,
                }
            }
            if revents.contains(PollFlags::POLLHUP) || revents.contains(PollFlags::POLLERR) {
                stdout_eof = true;
            }
            pf_idx += 1;
        }

        if !stderr_eof {
            let pf = if pf_idx < pollfds.len() { &pollfds[pf_idx] } else { continue };
            let revents = pf.revents().unwrap_or(PollFlags::empty());
            if revents.contains(PollFlags::POLLIN) {
                match nix::unistd::read(stderr_fd, &mut read_buf) {
                    Ok(0) => stderr_eof = true,
                    Ok(n) => {
                        last_activity = std::time::Instant::now();
                        let _ = tty.write_all(&read_buf[..n]);
                        if stderr_buf.len() < config.max_bytes {
                            let remaining = config.max_bytes - stderr_buf.len();
                            let take = n.min(remaining);
                            stderr_buf.extend_from_slice(&read_buf[..take]);
                            if n > remaining {
                                stderr_truncated = true;
                            }
                        }
                    }
                    Err(nix::errno::Errno::EIO) => stderr_eof = true,
                    Err(_) => stderr_eof = true,
                }
            }
            if revents.contains(PollFlags::POLLHUP) || revents.contains(PollFlags::POLLERR) {
                stderr_eof = true;
            }
        }
    }

    let ts_end = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Strip ANSI only — secret redaction is handled by `redtrail capture` via insert_command_redacted
    let stdout_clean = strip_ansi(&stdout_buf);
    let stderr_clean = strip_ansi(&stderr_buf);

    // Write temp files
    let out_path = format!("/tmp/rt-out-{}", config.shell_pid);
    let err_path = format!("/tmp/rt-err-{}", config.shell_pid);

    if !stdout_clean.is_empty() {
        write_capture_file(
            std::path::Path::new(&out_path),
            &TempFileHeader { ts_start, ts_end, truncated: stdout_truncated },
            &stdout_clean,
        )?;
    }

    if !stderr_clean.is_empty() {
        write_capture_file(
            std::path::Path::new(&err_path),
            &TempFileHeader { ts_start, ts_end, truncated: stderr_truncated },
            &stderr_clean,
        )?;
    }

    Ok(())
}
```

- [ ] **Step 4: Create `cmd::tee` entry point**

Create `src/cmd/tee.rs`:
```rust
use crate::core::tee;
use crate::core::capture::MAX_STDOUT_BYTES;
use crate::error::Error;

pub struct TeeArgs<'a> {
    pub session: &'a str,
    pub shell_pid: &'a str,
    pub ctl_fifo: &'a str,
    pub max_bytes: Option<usize>,
}

pub fn run(args: &TeeArgs) -> Result<(), Error> {
    tee::run_tee(&tee::TeeConfig {
        session_id: args.session.to_string(),
        shell_pid: args.shell_pid.to_string(),
        ctl_fifo: args.ctl_fifo.to_string(),
        max_bytes: args.max_bytes.unwrap_or(MAX_STDOUT_BYTES),
    })
}
```

- [ ] **Step 5: Register module and CLI subcommand**

In `src/cmd/mod.rs`, add:
```rust
pub mod tee;
```

In `src/cli.rs`, add the `Tee` subcommand:
```rust
/// PTY-aware output capture (called by shell hooks)
#[command(hide = true)]
Tee {
    #[arg(long)]
    session: String,
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
Commands::Tee { session, shell_pid, ctl_fifo, max_bytes } => {
    cmd::tee::run(&cmd::tee::TeeArgs {
        session: &session,
        shell_pid: &shell_pid,
        ctl_fifo: &ctl_fifo,
        max_bytes,
    })
}
```

- [ ] **Step 6: Run the CLI test**

Run: `cargo test --test tee_cli_test`
Expected: PASS — tee creates PTYs, writes paths to FIFO, relays output, creates temp files.

- [ ] **Step 7: Run all tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/core/tee.rs src/cmd/tee.rs src/cmd/mod.rs src/cli.rs tests/tee_cli_test.rs
git commit -m "feat: implement redtrail tee with PTY relay loop and CLI entry point"
```

---

## Task 7: Update shell hooks

**Files:**
- Modify: `src/cmd/init.rs`
- Modify: `tests/init_test.rs`

- [ ] **Step 1: Read existing init tests**

Read `tests/init_test.rs` to understand the current test structure.

- [ ] **Step 2: Write tests for new hook content**

Add to `tests/init_test.rs`:
```rust
#[test]
fn zsh_hook_contains_fifo_setup() {
    let output = redtrail_bin()
        .args(["init", "zsh"])
        .output()
        .expect("failed to run");

    let hook = String::from_utf8_lossy(&output.stdout);
    assert!(hook.contains("mkfifo"), "zsh hook should create FIFO");
    assert!(hook.contains("redtrail tee"), "zsh hook should launch redtrail tee");
    assert!(hook.contains("read -t 1"), "zsh hook should have FIFO read timeout");
    assert!(hook.contains("__RT_BLACKLIST"), "zsh hook should have inline blacklist");
    assert!(hook.contains("TRAPCHLD"), "zsh hook should have crash recovery");
    assert!(!hook.contains("date +%s%N"), "zsh hook should NOT use date +%s%N");
}

#[test]
fn bash_hook_contains_fifo_setup() {
    let output = redtrail_bin()
        .args(["init", "bash"])
        .output()
        .expect("failed to run");

    let hook = String::from_utf8_lossy(&output.stdout);
    assert!(hook.contains("mkfifo"), "bash hook should create FIFO");
    assert!(hook.contains("redtrail tee"), "bash hook should launch redtrail tee");
    assert!(hook.contains("read -t 1"), "bash hook should have FIFO read timeout");
    assert!(hook.contains("history 1"), "bash hook should use history 1 for full command");
    assert!(hook.contains("__RT_CAPTURE_ACTIVE"), "bash hook should have compound command guard");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --test init_test zsh_hook_contains_fifo`
Expected: FAIL — current hooks don't contain FIFO setup.

- [ ] **Step 4: Update the shell hook scripts in `src/cmd/init.rs`**

Replace the `ZSH_HOOK` and `BASH_HOOK` constants with the new versions from the spec (the full shell scripts from the "Shell Hook Changes" section of the design spec). These are the scripts that use FIFO-based PTY setup, `read -t 1` timeout, crash recovery, polling wait, and inline blacklist.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test init_test`
Expected: All init tests PASS.

- [ ] **Step 6: Run all tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/cmd/init.rs tests/init_test.rs
git commit -m "feat: update shell hooks with FIFO-based PTY capture setup"
```

---

## Task 8: End-to-end integration test

**Files:**
- Modify: `tests/tee_cli_test.rs`

This test exercises the full pipeline: tee → temp files → capture → DB.

- [ ] **Step 1: Write end-to-end test**

Add to `tests/tee_cli_test.rs`:
```rust
#[test]
fn end_to_end_tee_then_capture() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let _conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();

    let fifo_path = dir.path().join("ctl-fifo");
    nix::unistd::mkfifo(&fifo_path, nix::sys::stat::Mode::from_bits_truncate(0o600))
        .expect("mkfifo");

    let shell_pid = format!("e2e-{}", std::process::id());

    // Start tee
    let mut tee_child = redtrail_bin()
        .args([
            "tee",
            "--session", "e2e-sess",
            "--shell-pid", &shell_pid,
            "--ctl-fifo", fifo_path.to_str().unwrap(),
        ])
        .spawn()
        .expect("start tee");

    // Read PTY paths
    let fifo_content = std::fs::read_to_string(&fifo_path).unwrap();
    let paths: Vec<&str> = fifo_content.trim().split_whitespace().collect();

    // Simulate command writing to stdout PTY
    {
        let mut slave = std::fs::OpenOptions::new()
            .write(true)
            .open(paths[0])
            .unwrap();
        slave.write_all(b"build output line 1\nbuild output line 2\n").unwrap();
    }
    // Close stderr slave
    { let _ = std::fs::OpenOptions::new().write(true).open(paths[1]); }

    // Wait for tee to finish
    tee_child.wait().expect("tee wait");

    // Now run capture with the temp files
    let out_file = format!("/tmp/rt-out-{}", shell_pid);
    let output = redtrail_bin()
        .args([
            "capture",
            "--session-id", "e2e-sess",
            "--command", "make build",
            "--exit-code", "0",
            "--shell", "zsh",
            "--hostname", "devbox",
            "--stdout-file", &out_file,
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("capture");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    // Verify DB
    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(
        &conn,
        &redtrail::core::db::CommandFilter::default(),
    )
    .unwrap();

    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].command_raw, "make build");
    let stdout = cmds[0].stdout.as_ref().expect("stdout should be captured");
    assert!(stdout.contains("build output line 1"), "stdout: {stdout}");
    assert!(stdout.contains("build output line 2"), "stdout: {stdout}");
    assert!(cmds[0].timestamp_start > 0, "timestamp should come from tee");
    assert!(cmds[0].timestamp_end.is_some(), "ts_end should come from tee");

    // Temp files should be cleaned up by capture
    assert!(!std::path::Path::new(&out_file).exists());

    // Clean up any remaining
    let _ = std::fs::remove_file(format!("/tmp/rt-err-{}", shell_pid));
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test --test tee_cli_test end_to_end`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/tee_cli_test.rs
git commit -m "test: add end-to-end integration test for tee → capture → DB pipeline"
```

---

## Task 9: Final verification

- [ ] **Step 1: Run the full test suite**

Run: `cargo test`
Expected: All tests pass, zero failures.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -W clippy::all`
Expected: No warnings in new code.

- [ ] **Step 3: Verify binary builds**

Run: `cargo build --release`
Expected: Successful release build.

- [ ] **Step 4: Manual smoke test — tee subcommand**

```bash
# In one terminal:
mkfifo /tmp/rt-test-fifo
./target/release/redtrail tee --session test --shell-pid $$ --ctl-fifo /tmp/rt-test-fifo &

# Read the PTY paths:
read out_pty err_pty < /tmp/rt-test-fifo
echo "Got PTY paths: $out_pty $err_pty"

# Write to stdout PTY:
echo "hello from manual test" > "$out_pty"

# Close both (EOF triggers tee to write temp files):
exec 3>"$out_pty"; exec 3>&-
exec 4>"$err_pty"; exec 4>&-

wait

# Check temp files:
cat /tmp/rt-out-$$
# Should show: ts_start:..., ts_end:..., truncated:false, hello from manual test
rm /tmp/rt-out-$$ /tmp/rt-err-$$ 2>/dev/null
```

- [ ] **Step 5: Commit any final fixes**

Only if smoke test revealed issues. Otherwise, this task is done.
