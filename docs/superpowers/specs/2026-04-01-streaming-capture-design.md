# Streaming Capture for Long-Running Commands — Design Spec

> Approved: 2026-04-01
> Status: Ready for implementation planning
> Prerequisite for: Phase 2 Intelligent Extraction

---

## Problem

The current capture layer only writes to the DB when a command finishes (at `precmd`). Long-running commands — web servers (`rails s`, `npm run dev`), watchers (`cargo watch`), log tailers (`tail -f`) — produce valuable output that remains invisible until the process is killed. For a command running 8 hours, that's 8 hours of unrecorded port bindings, errors, and logs.

This blocks Phase 2 extraction from providing real-time context like "rails is listening on port 3000 right now."

## Solution

Split the capture lifecycle into `--start` and `--finish`. The tee process flushes output to the DB every 1 second during command execution, with secret redaction applied before each write.

---

## Current Flow

```
preexec:
  1. store command string, cwd, timestamp in shell vars
  2. spawn `redtrail tee` (background) — relays stdout/stderr to /dev/tty, buffers in memory
  3. redirect shell stdout/stderr to tee's PTY slaves

command runs... (nothing in DB)

precmd:
  1. restore fds
  2. signal tee (SIGUSR1) → tee writes temp files → tee exits
  3. call `redtrail capture` (sync/async) → reads temp files, inserts full row into DB
  4. delete temp files
```

## New Flow

```
preexec:
  1. call `redtrail capture start` (sync) → inserts MINIMAL command row (status='running'), prints command ID
     → only: session_id, command_raw, command_binary, cwd, timestamp_start, shell, hostname, source
     → NO git context, NO env snapshot (deferred to finish for speed)
  2. store command ID in shell var
  3. spawn `redtrail tee --command-id <id>` (background)
     → opens DB connection (via db::open) + loads redaction config
     → relays stdout/stderr to /dev/tty (unchanged)
     → every 1s, if new data since last flush:
         redact secrets → UPDATE stdout/stderr on command row
     → if on_detect=block and secrets found: stop writing to DB, delete the command row
  4. redirect shell stdout/stderr to tee's PTY slaves

command runs... (DB updated every 1s with redacted output)

precmd:
  1. restore fds
  2. signal tee (SIGUSR1) → tee does final flush → tee exits
  3. *** INVARIANT: the busy-wait on tee exit (while kill -0 ...) is LOAD-BEARING ***
     capture finish assumes tee is done writing. Removing this wait breaks correctness.
  4. call `redtrail capture finish --command-id <id> --exit-code $?`
     → fills in git_repo, git_branch, env_snapshot (deferred from start)
     → final redaction pass on full stdout/stderr (defense-in-depth)
     → sets exit_code, timestamp_end, status='finished'
     → syncs FTS index
     → enforces retention
     → zsh: runs backgrounded (&!)
     → bash: runs backgrounded (& disown)
```

---

## Component Changes

### 1. Shell Hooks (src/cmd/init.rs)

**zsh preexec changes:**

```zsh
__redtrail_preexec() {
    # ... blacklist check, escape hatch (unchanged) ...

    # NEW: create command row before tee starts
    __REDTRAIL_CMD_ID=$(command redtrail capture start \
        --session-id "$REDTRAIL_SESSION_ID" \
        --command "$1" \
        --cwd "$PWD" \
        --shell zsh \
        --hostname "${HOST:-$(hostname)}" \
        2>/dev/null)

    [[ -z "$__REDTRAIL_CMD_ID" ]] && return

    # Tee now receives command-id instead of just session
    local ctl_fifo="/tmp/rt-ctl-$$"
    mkfifo "$ctl_fifo" 2>/dev/null || return

    command redtrail tee \
        --command-id "$__REDTRAIL_CMD_ID" \
        --shell-pid "$$" \
        --ctl-fifo "$ctl_fifo" \
        2>/dev/null &
    __RT_TEE_PID=$!
    disown

    # ... PTY setup (unchanged) ...
}
```

**zsh precmd changes:**

```zsh
__redtrail_precmd() {
    local exit_code=$?

    # ... restore fds, signal tee, busy-wait for tee exit (unchanged) ...

    [[ -z "$__REDTRAIL_CMD_ID" ]] && return

    # Finish the command row (backgrounded — no prompt delay)
    # ts_end computed server-side, no `date` subprocess needed
    # cwd passed so capture finish can resolve git context
    command redtrail capture finish \
        --command-id "$__REDTRAIL_CMD_ID" \
        --exit-code "$exit_code" \
        --cwd "$__REDTRAIL_CWD" \
        2>/dev/null &!

    unset __REDTRAIL_CMD_ID __REDTRAIL_CWD __REDTRAIL_TS_START
    unset __RT_SAVE_OUT __RT_SAVE_ERR __RT_TEE_PID __RT_CAPTURE_ACTIVE
}
```

**bash precmd changes:**

```bash
__redtrail_precmd() {
    local exit_code=$?
    __RT_INSIDE_PRECMD=1

    # ... restore fds, signal tee, busy-wait for tee exit (unchanged) ...

    if [ -z "$__REDTRAIL_CMD_ID" ]; then
        unset __RT_INSIDE_PRECMD
        return
    fi

    # Background with disown (bash has no &!)
    command redtrail capture finish \
        --command-id "$__REDTRAIL_CMD_ID" \
        --exit-code "$exit_code" \
        --cwd "$__REDTRAIL_CWD" \
        2>/dev/null &
    disown

    unset __REDTRAIL_CMD_ID __REDTRAIL_CWD __REDTRAIL_TS_START
    unset __RT_SAVE_OUT __RT_SAVE_ERR __RT_TEE_PID __RT_CAPTURE_ACTIVE
    unset __RT_INSIDE_PRECMD
}
```

**bash DEBUG trap `$?` preservation:** The bash DEBUG trap must save `$?` at entry and avoid clobbering it:

```bash
__redtrail_preexec() {
    local saved_exit=$?   # preserve for PROMPT_COMMAND
    # ... blacklist check, capture start, tee spawn ...
    return $saved_exit    # restore $? for the user's command
}
```

### 2. CLI: `capture start` and `capture finish` (src/cmd/capture.rs + src/cli.rs)

The existing `Commands::Capture` variant is replaced by two subcommands under a `Capture` group:

```rust
#[derive(Subcommand)]
enum Commands {
    /// Record command execution (called by shell hooks)
    #[command(hide = true)]
    Capture {
        #[command(subcommand)]
        action: CaptureAction,
    },
    // ... other commands unchanged ...
}

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
        cwd: Option<String>,    // for git context resolution
    },
}
```

**`capture start` logic (must complete in <15ms):**
1. Load config, check `capture.enabled` → return empty if disabled
2. Parse command string → binary, subcommand, args, flags
3. Check blacklist → return empty string (no ID) if blacklisted
4. Detect source (in-process env check, <1ms)
5. Check `on_detect` mode:
   - If `block`: run secret redaction on `command_raw`. If secrets found, return empty (no capture).
   - If `redact`: redact `command_raw` before insert.
   - If `warn`: insert unredacted, log warning if secrets found.
6. Insert MINIMAL command row: `session_id`, `command_raw`, `command_binary`, `command_subcommand`, `cwd`, `timestamp_start`, `shell`, `hostname`, `source`, `status = 'running'`
   - **NO git context** (deferred to `capture finish` — git subprocesses take 10-30ms)
   - **NO env snapshot** (deferred to `capture finish`)
7. Print command ID to stdout (shell hook captures it)
8. Update session counters

**`capture finish` logic (runs backgrounded, no latency concern):**
1. Get git context from cwd (`git rev-parse`, `git branch` — 10-30ms, safe since backgrounded)
2. Build env snapshot
3. Read the command row by ID
4. If tee wrote stdout/stderr to the row, run a final full redaction pass (defense-in-depth for cross-chunk edge cases)
5. Handle `on_detect` modes on final stdout/stderr:
   - `block`: if secrets found in final content, DELETE the command row entirely
   - `warn`: log warning if secrets found, keep unredacted
   - `redact`: already redacted by tee, final pass catches any remaining
6. Update: `exit_code`, `timestamp_end`, `git_repo`, `git_branch`, `env_snapshot`, `status = 'finished'`
7. Compute `timestamp_end` server-side (`unixepoch()`) — no `date` subprocess needed in shell hook
8. Sync FTS index (INSERT into commands_fts)
9. Update session error_count if exit_code != 0
10. Enforce retention policy

### 3. Tee Process (src/core/tee.rs)

**New args:** `--command-id` replaces `--session`. The session ID is no longer needed by tee — it writes to a known command row.

**New behavior:**

```rust
pub struct TeeConfig {
    pub command_id: String,      // NEW: writes to this command row
    pub shell_pid: String,
    pub ctl_fifo: String,
    pub max_bytes: usize,
}
```

**Flush timer in the poll loop:**

The existing 250ms poll timeout stays. Add a `last_flush` timestamp and a `last_flush_len` tracker:

```rust
let flush_interval = Duration::from_secs(1);
let mut last_flush = Instant::now();
let mut last_flush_stdout_len: usize = 0;
let mut last_flush_stderr_len: usize = 0;
```

After each poll cycle (where data was received or timeout expired):

```rust
if last_flush.elapsed() >= flush_interval {
    let stdout_has_new = stdout_buf.len() > last_flush_stdout_len;
    let stderr_has_new = stderr_buf.len() > last_flush_stderr_len;

    if stdout_has_new || stderr_has_new {
        let stdout_clean = strip_ansi(&stdout_buf);
        let stderr_clean = strip_ansi(&stderr_buf);
        let stdout_redacted = redact(&stdout_clean);
        let stderr_redacted = redact(&stderr_clean);

        db_update_command_output(&conn, &command_id, &stdout_redacted, &stderr_redacted,
                                  stdout_truncated, stderr_truncated);

        last_flush_stdout_len = stdout_buf.len();
        last_flush_stderr_len = stderr_buf.len();
    }
    last_flush = Instant::now();
}
```

**On SIGUSR1 (final flush):** Same as periodic flush but unconditional. Then exit.

**DB connection:** Tee opens its own SQLite connection at startup. Uses the same `REDTRAIL_DB` env var / default path as the rest of the CLI. The connection uses WAL mode (already set by schema init), so concurrent reads from other CLI commands are safe.

**Secret redaction in tee:** Tee loads the config at startup to get the redaction settings and custom patterns file. It calls the same `redact_with_custom_patterns` function used by `capture`. This adds a dependency from `core::tee` to `core::secrets` and `config::Config`.

**`on_detect` modes in tee streaming:**

| Mode | Tee Behavior |
|---|---|
| `redact` | Redact secrets in the accumulated buffer before each DB write. Default. |
| `warn` | Write unredacted to DB. Log a single warning on first secret detection (not per-flush to avoid spam). |
| `block` | On first secret detection: DELETE the command row from DB, set an internal flag to stop future DB writes. Continue relaying to /dev/tty (don't break the terminal). `capture finish` checks if the row exists — if deleted, it's a no-op. |

**Temp file fallback removed.** Tee no longer writes temp files. All output goes directly to the DB. The `write_capture_file` / `read_capture_file` functions in `core/tee.rs` become dead code and are removed. `capture finish` no longer reads temp files.

### 4. DB Schema (src/core/db.rs)

Add `status` column to `commands`:

```sql
ALTER TABLE commands ADD COLUMN status TEXT NOT NULL DEFAULT 'finished';
```

Values: `'running'`, `'finished'`.

Migration: existing rows get `'finished'` via the DEFAULT. New rows inserted by `capture start` get `'running'`. `capture finish` updates to `'finished'`.

**New DB function — `update_command_output`:**

```rust
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
```

**New DB function — `finish_command`:**

```rust
pub fn finish_command(
    conn: &Connection,
    command_id: &str,
    exit_code: Option<i32>,
    git_repo: Option<&str>,
    git_branch: Option<&str>,
    env_snapshot: Option<&str>,
    stdout: Option<&str>,   // final redacted stdout (if tee populated it)
    stderr: Option<&str>,   // final redacted stderr
) -> Result<(), Error> {
    conn.execute(
        "UPDATE commands SET exit_code = ?1, timestamp_end = unixepoch(),
         git_repo = ?2, git_branch = ?3, env_snapshot = ?4,
         stdout = COALESCE(?5, stdout), stderr = COALESCE(?6, stderr),
         status = 'finished'
         WHERE id = ?7",
        rusqlite::params![exit_code, git_repo, git_branch, env_snapshot,
                          stdout, stderr, command_id],
    ).map_err(|e| Error::Db(e.to_string()))?;

    // Sync FTS index
    // ... (same pattern as current insert_command)
    Ok(())
}
```

**Impact on `insert_command`:** The existing `insert_command` function is refactored into `insert_command_start` which inserts a row with `status = 'running'` and no stdout/stderr/exit_code. The full `insert_command` stays for backward compat with tests and the `ingest` command, but sets `status = 'finished'`.

### 5. Compression Handling

Currently, `insert_command_compressed` handles large stdout by compressing it into `stdout_compressed` BLOB.

For streaming, tee writes plain text updates every 1s. Compression only happens at finalize time in `capture finish` — if the final stdout exceeds `max_stdout_bytes`, it gets compressed into `stdout_compressed` and `stdout` is set to NULL.

This means during execution, the `stdout` column holds plain text (up to `max_bytes`). After finalize, large outputs may move to `stdout_compressed`. This matches the existing read path which checks `stdout_compressed` as a fallback.

---

## What Changes for Existing Commands

### `redtrail status`

Can now show currently running commands:

```
Running commands:
  rails s     (2h14m, cwd: ~/myapp)
  npm run dev (45m, cwd: ~/frontend)
```

Query: `SELECT * FROM commands WHERE status = 'running'`

### `redtrail history`

No change — running commands appear with `exit_code = NULL`. The display can show "running" instead of an exit code.

### `redtrail context` (Phase 2)

Can show live state: "rails server is running on port 3000 right now" by querying running commands + extracted entities.

---

## Performance Budget

**`capture start` — CRITICAL PATH (sync in preexec, target: <15ms):**
- `db::open()` with pragmas + migration checks: ~5-10ms
- `parse_command()`: <1ms (in-process string parsing)
- `detect_source()`: <1ms (env var check)
- Secret redaction on `command_raw`: ~1ms
- INSERT minimal row: ~2-5ms (WAL mode)
- Print ID: <1ms
- **Total estimated: 10-18ms**
- **NOT included:** git context (10-30ms) — deferred to `capture finish`
- **NOT included:** env snapshot — deferred to `capture finish`

**Tee flush — BACKGROUND (every 1s):**
- ANSI strip + secret redaction: ~2ms
- UPDATE query: ~2-5ms
- Never blocks the user's terminal.

**`capture finish` — BACKGROUNDED (no latency concern):**
- git context: 10-30ms (2 subprocess forks)
- env snapshot: <1ms
- Read row + final redaction: ~5ms
- UPDATE + FTS insert: ~5ms
- Runs backgrounded in both zsh (`&!`) and bash (`& disown`).

**Net impact on prompt latency:** `capture start` adds ~10-15ms to `preexec` (before the user's command runs). The old flow added 0ms to preexec but ~20-40ms to precmd (after the command, before next prompt). The new flow's precmd is near-zero since `capture finish` is backgrounded. Overall, total perceived latency should be comparable or better.

---

## Error Handling

| Scenario | Behavior |
|---|---|
| `capture start` fails (DB locked, disk full) | Returns empty string. Shell hook sees empty ID, skips tee and finish. Command runs normally with no capture. |
| Tee can't open DB | Tee falls back to buffer-only mode (buffers in memory, does NOT write temp files). On SIGUSR1, exits. `capture finish` sees no stdout in DB row, no temp files — command row stays with NULL stdout. |
| Tee flush fails (DB locked briefly) | Logs warning internally, retries on next 1s cycle. SQLite WAL + busy_timeout=3000 makes this rare. |
| Shell killed (SIGKILL, crash) | Tee detects orphan (shell_pid check every 5s, already implemented). Does final flush and exits. Command row stays `status='running'` with partial stdout. |
| `capture finish` never called | Stale `running` rows. A periodic cleanup (or on next `capture start`) can mark old running commands as `status='orphaned'` if their `timestamp_start` is >24h old. |

### Orphan Cleanup

On every `capture start`, before inserting the new row:

```sql
UPDATE commands SET status = 'orphaned'
WHERE status = 'running'
AND session_id = ?
AND timestamp_start < unixepoch() - 86400;
```

This catches commands from crashed shells. The 24h threshold avoids marking legitimately long-running processes (servers, watchers) as orphaned.

---

## Files Changed

| File | Change |
|---|---|
| `src/cmd/init.rs` | Rewrite zsh and bash hooks for start/finish flow |
| `src/cmd/capture.rs` | Split into `start` and `finish` subcommand handlers |
| `src/cli.rs` | Replace `Commands::Capture` with subcommand group |
| `src/core/tee.rs` | Add DB connection, 1s flush timer, secret redaction, remove temp file writes |
| `src/cmd/tee.rs` | Update args: `--command-id` replaces `--session` |
| `src/core/db.rs` | Add `status` column migration, `insert_command_start`, `update_command_output`, `finish_command` |
| `src/core/capture.rs` | No structural changes — shared logic (parse, blacklist, git_context) already lives here. Both start/finish call into it. |

### Dead Code Removal

- `write_capture_file` and `read_capture_file` in `core/tee.rs` — no longer needed
- `TempFileHeader` struct — no longer needed
- Temp file reading logic in `cmd/capture.rs` — replaced by DB reads

---

## Live Test Strategy

### Existing Tests to Update

All tests that source `redtrail init zsh/bash` will pick up the new hooks automatically. Tests that verify capture behavior need the following adjustments:

| Test | Change Needed |
|---|---|
| `capture-basic.sh` | Verify still works with new hook flow. Check `status='finished'` in DB. |
| `capture-basic-bash.sh` | Same as above for bash hooks. |
| `capture-stdout.sh` | Verify stdout is captured. Query may need to account for streaming (stdout populated by tee, not capture). |
| `capture-stdout-bash.sh` | Same as above for bash. |
| `capture-compress-stdout.sh` | Verify compression still happens at finalize. |
| `capture-disabled.sh` | Verify `capture start` respects `capture.enabled = false`. |
| `capture-custom-blacklist.sh` | Verify `capture start` returns empty ID for blacklisted commands. |
| All `redact-*.sh` tests | Verify redaction works in streaming path. |
| All `capture-on-detect-*.sh` | Verify on_detect modes work with new flow. |

### New Live Tests

**`capture-streaming.sh`** — Core streaming test:
```bash
# Run a command that produces output over multiple seconds
# Verify DB row exists with status='running' and partial stdout DURING execution
# Verify status='finished' and complete stdout AFTER execution

# 1. Start a backgrounded loop that outputs lines with sleeps
cat >"$TMPDIR/commands.txt" <<'EOF'
for i in 1 2 3; do echo "line-$i"; sleep 1; done &
BGPID=$!
sleep 2
# Query DB for running/partial stdout while loop is still going
/usr/local/bin/redtrail query "SELECT status, stdout FROM commands WHERE command_raw LIKE '%line-%'" --json > /tmp/mid-check.json
wait $BGPID
sleep 1
exit
EOF

# After shell exits:
# Verify mid-check.json shows the command with partial stdout
# Verify final DB state has status='finished' and all 3 lines
```

**`capture-streaming-redact.sh`** — Secrets redacted during streaming:
```bash
# Run a command that outputs a secret pattern over time
# Verify the DB never contains the raw secret, even during execution

# 1. Echo an AWS key pattern with a delay
# 2. Query DB mid-execution
# 3. Verify [REDACTED:...] appears, not the raw key
```

**`capture-streaming-port.sh`** — Server port detection:
```bash
# Start a simple HTTP server (python -m http.server)
# Verify DB has stdout containing the port announcement within 2-3 seconds
# Kill the server, verify status='finished'
```

**`capture-orphan-cleanup.sh`** — Orphaned running commands:
```bash
# Manually insert a stale 'running' command from >24h ago
# Run a new command (triggers capture start with orphan cleanup)
# Verify the stale command is now status='orphaned'
```

**`capture-streaming-block.sh`** — `on_detect=block` with streaming:
```bash
# Configure on_detect=block
# Run a command that outputs a secret pattern after a delay
# Verify the command row is deleted from DB after tee detects the secret
# Verify capture finish is a no-op (no row to update)
```

**`capture-streaming-warn.sh`** — `on_detect=warn` with streaming:
```bash
# Configure on_detect=warn
# Run a command that outputs a secret pattern
# Verify stdout is stored unredacted in DB
# Verify only a single warning (not per-flush spam)
```

**`capture-rapid-commands.sh`** — Rapid sequential commands:
```bash
# Run 10+ fast commands in quick succession (ls, echo, pwd, etc.)
# Verify all are captured correctly in DB with status='finished'
# Verify no duplicate or missing command IDs
```

**`capture-tee-crash.sh`** — Tee killed mid-stream:
```bash
# Start a long-running command
# Kill the tee process (SIGKILL) while it's running
# Verify the command row exists with partial stdout from last successful flush
# Verify capture finish still completes and sets status='finished'
```

---

## Acceptance Criteria

### Core Lifecycle
- [ ] `capture start` inserts a minimal `status='running'` row and prints the command ID
- [ ] `capture start` completes in <15ms (no git subprocesses, no env snapshot)
- [ ] `capture finish` fills in git_repo, git_branch, env_snapshot, exit_code, timestamp_end
- [ ] `capture finish` sets status='finished'
- [ ] `capture finish` runs backgrounded in both zsh (`&!`) and bash (`& disown`)
- [ ] Busy-wait on tee exit in precmd is implemented (load-bearing for correctness)

### Streaming
- [ ] Tee flushes stdout/stderr to DB every 1s when new data is available
- [ ] Tee writes full accumulated buffer on each flush (not deltas)
- [ ] FTS index is synced at `capture finish` (not during streaming flushes)
- [ ] Compression of large stdout happens at `capture finish`, not during streaming

### Security
- [ ] Secret redaction runs on every tee flush (on_detect=redact) — no unredacted secrets in DB
- [ ] `capture finish` runs a final full-content redaction pass (defense-in-depth)
- [ ] on_detect=block: tee DELETEs command row on first secret detection, stops DB writes
- [ ] on_detect=warn: tee logs a single warning on first secret detection, stores unredacted
- [ ] on_detect=block in `capture start`: returns empty ID if command_raw contains secrets

### Robustness
- [ ] Running commands visible via `SELECT * FROM commands WHERE status = 'running'`
- [ ] Orphaned running commands (>24h) are cleaned up on next `capture start`
- [ ] Shell hooks work for both zsh and bash
- [ ] Bash DEBUG trap preserves `$?` across `capture start` calls
- [ ] If `capture start` fails, the command runs normally with no capture
- [ ] If tee can't reach DB, it still relays output to /dev/tty (terminal not broken)
- [ ] If tee is killed mid-stream, partial stdout is preserved, `capture finish` still completes

### Tests
- [ ] All existing live tests pass with the new hook flow
- [ ] New live tests pass: streaming, streaming-redact, streaming-port, orphan-cleanup
- [ ] New live tests pass: streaming-block, streaming-warn, rapid-commands, tee-crash
- [ ] No temp files (`/tmp/rt-out-*`, `/tmp/rt-err-*`) are created in the new flow
