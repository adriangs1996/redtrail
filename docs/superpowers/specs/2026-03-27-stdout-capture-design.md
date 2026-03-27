# Stdout/Stderr Capture via `redtrail tee`

> Phase 1 — Silent Capture
> Date: 2026-03-27

---

## Problem

RedTrail captures command metadata (command string, exit code, cwd, timing) but not stdout/stderr. The DB schema supports it, the `NewCommand` struct accepts it, but the shell hooks never pass output data. Without stdout/stderr, full-text search across output is empty, extraction in Phase 2 has nothing to parse, and error-resolution mapping in Phase 3 has no error messages to work with.

Capturing stdout/stderr from a shell hook is hard because:

- **eval-based wrapping** breaks quoting semantics and double-expands variables
- **Pipe-based tee** kills TTY detection — commands lose colors, pagers, progress bars
- **Process substitution** creates pipes, not PTYs — `isatty()` returns false even if the tee process allocates a PTY internally, because the command writes to the pipe fd, not the PTY slave fd
- **Command rewriting** (e.g., `accept-line` in zsh) pollutes shell history in bash, can't handle multi-line commands in bash (`bind -x` fires per-line), and requires maintaining two different architectures
- **Process substitution race conditions** — tee may not flush before precmd reads the file

---

## Solution: fd-dup + `redtrail tee` with PTY slave exposure

A single architecture for both zsh and bash:

1. **preexec/DEBUG trap:** Launch `redtrail tee` which creates PTY pairs and exposes slave paths via a FIFO. Shell redirects stdout/stderr to the PTY slave fds directly — commands see `isatty() == true`.
2. **precmd/PROMPT_COMMAND:** Restore original fds, closing the PTY slaves. `redtrail tee` sees EOF on the master side, writes captured output to temp files, and exits.
3. **`redtrail capture`** reads the temp files via `--stdout-file`/`--stderr-file` flags, stores them with the command row, and deletes the temp files. Single writer, single reader, no race.

### Why this works

- **No command rewriting** — the shell executes the user's command verbatim
- **No eval** — no quoting/expansion issues
- **No history pollution** — hooks are invisible
- **Multi-line commands work** — we don't parse or transform the command
- **One architecture** — identical approach for zsh and bash
- **TTY truly preserved** — the shell redirects stdout to the PTY slave fd directly, so `isatty()` returns true for the command's stdout/stderr
- **No race condition** — `redtrail tee` writes to temp files, `redtrail capture` reads them sequentially
- **Timestamps in Rust** — `std::time::SystemTime` provides nanosecond precision on all platforms, avoiding `date +%s%N` which is broken on macOS <26

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                         USER'S SHELL                                 │
│                                                                      │
│  preexec:                                                            │
│    mkfifo /tmp/rt-ctl-$$                                             │
│    redtrail tee --session $SID --shell-pid $$ --ctl /tmp/rt-ctl-$$ & │
│    read -t 1 pty_out pty_err < /tmp/rt-ctl-$$  (1s timeout)         │
│    rm -f /tmp/rt-ctl-$$                                              │
│    exec {RT_SAVE_OUT}>&1 {RT_SAVE_ERR}>&2                           │
│    exec 1>$pty_out 2>$pty_err                                        │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │               USER'S COMMAND RUNS                              │  │
│  │                                                                │  │
│  │  stdout fd ──→ PTY slave (OS-dependent path) ──→ isatty()=true │  │
│  │  stderr fd ──→ PTY slave (OS-dependent path) ──→ isatty()=true │  │
│  │                                                                │  │
│  │  redtrail tee reads both PTY masters:                          │  │
│  │    ├──→ writes to /dev/tty (real terminal — user sees output)  │  │
│  │    └──→ accumulates in per-stream capture buffers              │  │
│  └────────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  precmd:                                                             │
│    exec 1>&${RT_SAVE_OUT} 2>&${RT_SAVE_ERR}                         │
│    exec {RT_SAVE_OUT}>&- {RT_SAVE_ERR}>&-                            │
│    (PTY slaves closed → master EOF → tee writes temp files, exits)   │
│    wait $__RT_TEE_PID (with polling timeout)                         │
│    redtrail capture ... --stdout-file /tmp/rt-out-$$ \               │
│                         --stderr-file /tmp/rt-err-$$                 │
│    (capture reads files, stores in DB, deletes temp files)           │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

---

## `redtrail tee` Binary Design

### Invocation

```
redtrail tee --session <session-id> --shell-pid <pid> --ctl-fifo <path> [--max-bytes 51200]
```

Single process handles both stdout and stderr PTYs. Timestamps are generated internally via `std::time::SystemTime` — the shell does not pass timestamps.

### Behavior

1. **Allocate two PTY pairs** — one for stdout, one for stderr. Each has a master (read side) and slave (write side).
2. **Initialize PTY window size** — query the current terminal dimensions from `/dev/tty` via `ioctl(TIOCGWINSZ)` and set them on both PTY slaves via `ioctl(TIOCSWINSZ)`. This must happen before writing slave paths to the FIFO, so commands that check terminal size on startup get correct dimensions.
3. **Write slave paths to FIFO** — write the two slave paths (space-separated) to `--ctl-fifo`. Paths are OS-dependent: `/dev/pts/N` on Linux, `/dev/ttysNNN` on macOS. This unblocks the shell's `read -t 1` call.
4. **Relay loop** — poll both PTY masters. For each chunk:
   - Write to `/dev/tty` (the real controlling terminal, bypassing any fd redirection)
   - Append to the corresponding capture buffer (stdout or stderr), bounded by `--max-bytes`
5. **On EOF** (both masters see EOF after shell closes the slave fds):
   - Strip ANSI escape sequences from captured buffers
   - Run secret redaction on the cleaned buffers
   - Write temp files with mode 0600 (owner read/write only):
     - `/tmp/rt-out-<shell-pid>` — stdout capture
     - `/tmp/rt-err-<shell-pid>` — stderr capture
   - Exit with code 0
6. **On PTY allocation failure** — exit immediately without writing to the FIFO. The shell's `read -t 1` times out and skips capture gracefully.
7. **Signal handling:**
   - **SIGWINCH** — query new size from `/dev/tty`, forward to both PTY slaves via `ioctl(TIOCSWINSZ)`
   - **SIGINT/SIGTERM** — write whatever is in the buffer to temp files before exiting, so partial output is still captured
   - **SIGHUP** — same as SIGTERM

### Capture Buffer

- Default max: 50KB (`MAX_STDOUT_BYTES` already defined in `capture.rs`)
- Separate buffers for stdout and stderr, each independently bounded
- When a buffer exceeds max, stop accumulating but keep relaying to `/dev/tty`
- Record truncation status per-stream in the temp file header

### PTY Allocation

Use the `nix` crate for POSIX PTY operations:

- `posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY)` — create master
- `grantpt()` / `unlockpt()` — prepare slave
- `ptsname_r()` — get slave path (returns OS-specific path: `/dev/pts/N` on Linux, `/dev/ttysNNN` on macOS)

The shell opens the slave path directly via `exec 1>$pty_out`. The command inherits this fd. `isatty(1)` returns `true` because the fd points to a real PTY device.

`redtrail tee` holds the master fds and reads output from them.

### ANSI Stripping

Use the `strip-ansi-escapes` crate — it handles CSI, OSC, DCS sequences, cursor save/restore, and title-setting escapes. Do not use a regex; it will miss non-CSI escapes.

### Relay Target: `/dev/tty`

`redtrail tee` writes relayed output to `/dev/tty`, not to its inherited stdout. This is important because:

- The process is backgrounded (`&`) — its stdout may be redirected or detached
- `/dev/tty` always refers to the controlling terminal of the session
- This ensures the user sees output regardless of how the tee process was spawned

---

## DB Write Strategy: Temp File Handoff

No race condition. No retry logic. No concurrent DB writers.

1. `redtrail tee` writes captured output to temp files (mode 0600) on EOF:
   - `/tmp/rt-out-<shell-pid>` — stdout capture (ANSI-stripped, secret-redacted)
   - `/tmp/rt-err-<shell-pid>` — stderr capture (ANSI-stripped, secret-redacted)
   - Files include a header line: `truncated:true` or `truncated:false`
2. Shell hook waits for tee to exit (polling with timeout — see shell hooks below)
3. Shell hook calls `redtrail capture --stdout-file /tmp/rt-out-$$ --stderr-file /tmp/rt-err-$$ ...`
4. `redtrail capture` reads the files, inserts the complete command row (metadata + output) in one DB operation, then deletes the temp files
5. No shell-side `rm` needed — `redtrail capture` owns cleanup

This is sequential: tee finishes → capture reads → capture cleans up → done. One DB writer, one transaction.

### Temp File Format

```
truncated:false
<captured output content>
```

First line is metadata. Rest is the captured content. If the file doesn't exist or is empty, that stream had no output — capture stores NULL.

### Temp File Security

Temp files are created with mode 0600 (`open()` with `O_CREAT | O_WRONLY`, mode `0o600`). This matches the CLAUDE.md mandate that database file permissions must be 600. Even though content is secret-redacted before writing, the output may contain sensitive-but-not-pattern-matched information.

---

## Timestamp Strategy

**All timestamps are generated in Rust, not in the shell.**

- `redtrail tee` records `ts_start` (when the PTY is ready and the FIFO handshake completes) and `ts_end` (when EOF is received). It writes these to the temp file headers.
- `redtrail capture` reads the timestamps from temp file headers and uses them for the DB record.
- When no temp files exist (capture-only, no tee), `redtrail capture` generates `ts_start` itself using `std::time::SystemTime`.
- This eliminates the dependency on `date +%s%N` which outputs a literal `N` on macOS versions prior to macOS 26 (Tahoe). Stock BSD `date` on macOS 13-15 (the majority of dev machines) does not support `%N`.

### Temp File Header (updated)

```
ts_start:1711555200000000000
ts_end:1711555203500000000
truncated:false
<captured output content>
```

All timestamps are nanoseconds since Unix epoch. Header is line-based, terminated by the first blank line.

---

## Shell Hook Changes

### zsh

```zsh
# RedTrail shell integration — zsh
# eval "$(redtrail init zsh)"

export REDTRAIL_SESSION_ID="$(command redtrail session-id 2>/dev/null || echo "rt-$$-$(date +%s)")"

# Inline blacklist — no subprocess per command
__RT_BLACKLIST=":vim:nvim:nano:vi:ssh:scp:top:htop:btop:less:more:man:tmux:screen:"

__redtrail_preexec() {
    __REDTRAIL_CMD="$1"
    __REDTRAIL_CWD="$PWD"
    __RT_CAPTURE_ACTIVE=""

    # Blacklist check — extract binary, handle path-qualified and env-prefixed
    local cmd_str="$1"
    # Strip leading env assignments: FOO=bar BAZ=qux cmd ...
    while :; do
        local word="${cmd_str%% *}"
        [[ "$word" == *=* ]] || break
        cmd_str="${cmd_str#"$word" }"
    done
    local binary="${cmd_str%% *}"
    binary="${binary##*/}"  # basename: /usr/bin/vim → vim

    if [[ "$__RT_BLACKLIST" == *":$binary:"* ]]; then
        return
    fi

    # Set up capture via redtrail tee
    local ctl_fifo="/tmp/rt-ctl-$$"
    mkfifo "$ctl_fifo" 2>/dev/null || return

    command redtrail tee \
        --session "$REDTRAIL_SESSION_ID" \
        --shell-pid "$$" \
        --ctl-fifo "$ctl_fifo" \
        2>/dev/null &
    __RT_TEE_PID=$!

    local pty_out pty_err
    if ! read -t 1 pty_out pty_err < "$ctl_fifo"; then
        # Timeout or error — tee failed to start, skip capture
        rm -f "$ctl_fifo"
        kill "$__RT_TEE_PID" 2>/dev/null
        return
    fi
    rm -f "$ctl_fifo"

    # Redirect stdout/stderr to PTY slaves
    exec {__RT_SAVE_OUT}>&1 {__RT_SAVE_ERR}>&2
    exec 1>"$pty_out" 2>"$pty_err"
    __RT_CAPTURE_ACTIVE=1
}

__redtrail_precmd() {
    local exit_code=$?

    # Restore fds — always, even if capture setup partially failed
    if [[ -n "$__RT_CAPTURE_ACTIVE" ]]; then
        exec 1>&${__RT_SAVE_OUT} 2>&${__RT_SAVE_ERR}
        exec {__RT_SAVE_OUT}>&- {__RT_SAVE_ERR}>&-

        # Wait for tee with polling timeout (max 500ms)
        local i
        for i in 1 2 3 4 5; do
            kill -0 "$__RT_TEE_PID" 2>/dev/null || break
            sleep 0.1
        done
    fi

    [[ -z "$__REDTRAIL_CMD" ]] && return

    local stdout_arg="" stderr_arg=""
    local out_file="/tmp/rt-out-$$" err_file="/tmp/rt-err-$$"
    [[ -f "$out_file" ]] && stdout_arg="--stdout-file $out_file"
    [[ -f "$err_file" ]] && stderr_arg="--stderr-file $err_file"

    # capture runs sync, reads temp files, inserts to DB, deletes temp files
    command redtrail capture \
        --session-id "$REDTRAIL_SESSION_ID" \
        --command "$__REDTRAIL_CMD" \
        --cwd "$__REDTRAIL_CWD" \
        --exit-code "$exit_code" \
        --shell zsh \
        --hostname "${HOST:-$(hostname)}" \
        $stdout_arg $stderr_arg \
        2>/dev/null &!

    unset __REDTRAIL_CMD __REDTRAIL_CWD
    unset __RT_SAVE_OUT __RT_SAVE_ERR __RT_TEE_PID __RT_CAPTURE_ACTIVE
}

# Crash recovery: if tee dies, restore fds immediately
# Note: TRAPCHLD fires for ALL child exits. PID reuse is theoretically
# possible but extremely unlikely in the short window between tee death
# and SIGCHLD delivery. Accepted low-probability edge case.
TRAPCHLD() {
    if [[ -n "$__RT_CAPTURE_ACTIVE" ]] && ! kill -0 "$__RT_TEE_PID" 2>/dev/null; then
        exec 1>&${__RT_SAVE_OUT} 2>&${__RT_SAVE_ERR} 2>/dev/null
        exec {__RT_SAVE_OUT}>&- {__RT_SAVE_ERR}>&- 2>/dev/null
        __RT_CAPTURE_ACTIVE=""
    fi
}

autoload -Uz add-zsh-hook
add-zsh-hook preexec __redtrail_preexec
add-zsh-hook precmd __redtrail_precmd
```

### bash

```bash
# RedTrail shell integration — bash
# eval "$(redtrail init bash)"

export REDTRAIL_SESSION_ID="$(command redtrail session-id 2>/dev/null || echo "rt-$$-$(date +%s)")"

# Inline blacklist
__RT_BLACKLIST=":vim:nvim:nano:vi:ssh:scp:top:htop:btop:less:more:man:tmux:screen:"

__redtrail_preexec() {
    [ -n "$COMP_LINE" ] && return
    [ "$BASH_COMMAND" = "${PROMPT_COMMAND%%;*}" ] && return
    [ -n "$__RT_INSIDE_PRECMD" ] && return
    [ -n "$__RT_CAPTURE_ACTIVE" ] && return  # compound command guard

    # Use history for full pipeline (BASH_COMMAND only has current simple command)
    __REDTRAIL_CMD="$(HISTTIMEFORMAT= history 1 | sed 's/^[ ]*[0-9]*[ ]*//')"
    __REDTRAIL_CWD="$PWD"

    # Blacklist check
    local cmd_str="$__REDTRAIL_CMD"
    while :; do
        local word="${cmd_str%% *}"
        [[ "$word" == *=* ]] || break
        cmd_str="${cmd_str#"$word" }"
    done
    local binary="${cmd_str%% *}"
    binary="${binary##*/}"

    if [[ ":$__RT_BLACKLIST:" == *":$binary:"* ]]; then
        return
    fi

    # Set up capture
    local ctl_fifo="/tmp/rt-ctl-$$"
    mkfifo "$ctl_fifo" 2>/dev/null || return

    command redtrail tee \
        --session "$REDTRAIL_SESSION_ID" \
        --shell-pid "$$" \
        --ctl-fifo "$ctl_fifo" \
        2>/dev/null &
    __RT_TEE_PID=$!

    local pty_out pty_err
    if ! read -t 1 pty_out pty_err < "$ctl_fifo"; then
        rm -f "$ctl_fifo"
        kill "$__RT_TEE_PID" 2>/dev/null
        return
    fi
    rm -f "$ctl_fifo"

    exec {__RT_SAVE_OUT}>&1 {__RT_SAVE_ERR}>&2
    exec 1>"$pty_out" 2>"$pty_err"
    __RT_CAPTURE_ACTIVE=1
}

__redtrail_precmd() {
    local exit_code=$?
    __RT_INSIDE_PRECMD=1

    if [[ -n "$__RT_CAPTURE_ACTIVE" ]]; then
        exec 1>&${__RT_SAVE_OUT} 2>&${__RT_SAVE_ERR}
        exec {__RT_SAVE_OUT}>&- {__RT_SAVE_ERR}>&-

        # Wait for tee with polling timeout (max 500ms)
        local i
        for i in 1 2 3 4 5; do
            kill -0 "$__RT_TEE_PID" 2>/dev/null || break
            sleep 0.1
        done
    fi

    if [ -z "$__REDTRAIL_CMD" ]; then
        unset __RT_INSIDE_PRECMD
        return
    fi

    local stdout_arg="" stderr_arg=""
    local out_file="/tmp/rt-out-$$" err_file="/tmp/rt-err-$$"
    [[ -f "$out_file" ]] && stdout_arg="--stdout-file $out_file"
    [[ -f "$err_file" ]] && stderr_arg="--stderr-file $err_file"

    # capture runs SYNC in bash — no backgrounding, so it can safely
    # read and delete temp files before returning
    command redtrail capture \
        --session-id "$REDTRAIL_SESSION_ID" \
        --command "$__REDTRAIL_CMD" \
        --cwd "$__REDTRAIL_CWD" \
        --exit-code "$exit_code" \
        --shell bash \
        --hostname "${HOSTNAME:-$(hostname)}" \
        $stdout_arg $stderr_arg \
        2>/dev/null

    unset __REDTRAIL_CMD __REDTRAIL_CWD
    unset __RT_SAVE_OUT __RT_SAVE_ERR __RT_TEE_PID __RT_CAPTURE_ACTIVE
    unset __RT_INSIDE_PRECMD
}

trap '__redtrail_preexec' DEBUG
PROMPT_COMMAND="__redtrail_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
```

### Shell-specific notes

**Timestamps:** Shell hooks no longer pass `--ts-start` or `--ts-end`. `redtrail tee` generates nanosecond timestamps internally via Rust's `std::time::SystemTime` and writes them to the temp file headers. `redtrail capture` reads them from the headers. When capture runs without temp files (no tee), it generates its own timestamp. This avoids the `date +%s%N` portability problem — stock BSD `date` on macOS 13-15 outputs a literal `N` for `%N`.

**Bash full command capture:** Bash's `$BASH_COMMAND` only contains the current simple command in a pipeline. `cat foo | grep bar` gives `cat foo` on first DEBUG trap fire. We use `history 1` instead to get the full command line as the user typed it. The `HISTTIMEFORMAT=` prefix strips timestamp formatting.

**Bash compound command guard:** The `__RT_CAPTURE_ACTIVE` flag prevents the DEBUG trap from re-entering on subsequent simple commands within a compound command (`cmd1 && cmd2`). The combined stdout/stderr of the entire compound command is captured as a single blob.

**Bash sync capture:** In bash, `redtrail capture` runs synchronously (not backgrounded) so it can safely read and delete temp files. In zsh, capture is backgrounded with `&!` for lower precmd latency — this is safe because `&!` disowns the process, and `redtrail capture` reads the files immediately on startup before they could be overwritten by a subsequent command's tee.

**No SIGCHLD trap in bash:** Bash's trap mechanism is less flexible than zsh's TRAPCHLD. If `redtrail tee` crashes mid-command in bash, output goes dark until precmd fires. This is an accepted limitation — precmd always restores fds. Long-running commands are the risk window. A future improvement could poll `kill -0` on a short timer.

---

## Crash Recovery

### zsh

Zsh's `TRAPCHLD` fires when any child process exits. The handler checks if the tee process died and immediately restores fds:

```zsh
TRAPCHLD() {
    if [[ -n "$__RT_CAPTURE_ACTIVE" ]] && ! kill -0 "$__RT_TEE_PID" 2>/dev/null; then
        exec 1>&${__RT_SAVE_OUT} 2>&${__RT_SAVE_ERR} 2>/dev/null
        exec {__RT_SAVE_OUT}>&- {__RT_SAVE_ERR}>&- 2>/dev/null
        __RT_CAPTURE_ACTIVE=""
    fi
}
```

This means: if `redtrail tee` crashes during a `cargo build`, output resumes within milliseconds (next SIGCHLD delivery). The user sees a brief gap, not a blank terminal.

**PID reuse edge case:** If `__RT_TEE_PID` is recycled to another process between tee's death and SIGCHLD delivery, `kill -0` sees the new process and doesn't restore fds. This requires: tee dies, the OS recycles that exact PID, and SIGCHLD is delivered — all within the same command execution window. Extremely low probability. Accepted.

### bash

No equivalent SIGCHLD handler that works reliably during command execution. Accepted limitation — precmd always restores fds. The window of risk is the duration of the current command.

---

## Blacklist Handling

The inline blacklist check handles:

- **Simple commands:** `vim file` — extracts `vim`
- **Path-qualified:** `/usr/bin/vim file` — extracts `vim` via `${binary##*/}`
- **Env-prefixed:** `EDITOR=vim FOO=bar vim file` — strips `VAR=val` prefixes via `while :; do ... break` loop
- **`command`/`builtin` prefixes:** Not handled — `command vim` extracts `command`. Acceptable: `command vim` is rare and the user would see normal behavior (just no stdout capture for that invocation).

---

## Signal Handling

| Signal           | Behavior                                                                                                                                                                                                                                                            |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| SIGWINCH         | `redtrail tee` queries new size from `/dev/tty` via `ioctl(TIOCGWINSZ)`, forwards to both PTY slaves via `ioctl(TIOCSWINSZ)`. Commands handle terminal resize correctly.                                                                                            |
| SIGINT (Ctrl+C)  | Delivered to the foreground process group. `redtrail tee` catches it, writes partial buffer to temp files, exits. Shell restores fds in precmd.                                                                                                                     |
| SIGTSTP (Ctrl+Z) | Foreground command is suspended. Shell's precmd fires, restores fds, closes slaves. Tee sees EOF, writes what it has. If the user later `fg`s the job, it writes to the original (restored) stdout — no tee in the path. Partial capture only. Accepted limitation. |
| SIGTERM/SIGHUP   | `redtrail tee` writes buffer to temp files before exiting                                                                                                                                                                                                           |

### Known limitation: `exec` replacing shell

If the user runs `exec zsh` (replacing their shell), preexec fires but precmd never does. `redtrail tee` holds the PTY master open indefinitely. Mitigation: `redtrail tee` has an inactivity timeout (default: 5 minutes). If no data flows through the PTY for that long, it writes what it has and exits.

---

## Schema Changes

### New column: `stderr_truncated`

```sql
ALTER TABLE commands ADD COLUMN stderr_truncated BOOLEAN DEFAULT 0;
```

The existing `stdout_truncated` column tracks stdout only. stderr needs its own flag since they are independently bounded buffers.

### Timestamp precision

Timestamps are now nanoseconds since Unix epoch (generated in Rust via `std::time::SystemTime`). Existing data stored as seconds will appear as very old timestamps — acceptable since we're pre-release. No migration needed.

**Important:** The `(session_id, timestamp_start)` pair is no longer used for DB lookups (the temp file approach eliminated that need), so the precision change is purely for accuracy.

---

## Failure Modes

| Failure                            | Impact                                                   | Mitigation                                                            |
| ---------------------------------- | -------------------------------------------------------- | --------------------------------------------------------------------- |
| `redtrail tee` binary missing      | `mkfifo` succeeds but `read -t 1` times out              | 1-second timeout on FIFO read, skip capture, kill tee PID             |
| `redtrail tee` crashes mid-command | Output goes dark until SIGCHLD (zsh) or precmd (bash)    | TRAPCHLD restores fds in zsh; precmd always restores in both          |
| PTY allocation fails               | `redtrail tee` exits without writing to FIFO             | `read -t 1` times out, shell skips capture                           |
| FIFO creation fails (`/tmp` full)  | `mkfifo` returns error, preexec returns early            | No capture for that command, no user impact                           |
| DB write fails in capture          | Command metadata still stored, output field is NULL      | Log error, user unaffected                                            |
| Temp file write fails              | `redtrail capture` sees no file, stores NULL for output  | No capture for that command                                           |
| Very large output (>50KB)          | Buffer capped per-stream, rest relayed but not stored    | `stdout_truncated`/`stderr_truncated = true`                          |
| Binary output                      | Stored as-is up to limit, not useful for extraction      | Future: detect binary content and skip storage                        |
| `redtrail tee` hangs (no EOF)      | Polling wait times out after 500ms, precmd continues     | Inactivity timeout in tee (5min) eventually cleans up                 |

**Key invariant:** If any part of the capture pipeline fails, the user's terminal behavior is unchanged. Every failure path restores fds and continues. Capture is best-effort.

---

## New Dependencies

- `nix` crate — POSIX PTY operations (`posix_openpt`, `grantpt`, `unlockpt`, `ptsname_r`), signal handling, `ioctl` for TIOCGWINSZ/TIOCSWINSZ
- `strip-ansi-escapes` crate — handles CSI, OSC, DCS sequences and all terminal escape codes

---

## Performance Budget

- **preexec overhead:** `mkfifo` + fork `redtrail tee` + block on FIFO read (up to 1s timeout, typically <10ms) + two `exec` redirects. Target: <25ms total.
- **precmd overhead:** Two `exec` fd restores + polling wait (max 500ms, typically <10ms) + one `redtrail capture` spawn + temp file reads. Target: <15ms.
- **`redtrail tee` relay latency:** PTY read → `/dev/tty` write should add <1ms per chunk.
- **`redtrail tee` process lifetime:** Lives for the duration of the command. Uses <2MB memory (two capture buffers + two PTYs). Exits on EOF.
- **Total per-command budget:** <50ms overhead (preexec + precmd combined), consistent with CLAUDE.md mandate.

Single `redtrail tee` process for both streams avoids the cost of two Rust binary spawns.

---

## Scope

### In scope

- `redtrail tee` binary — single process, two PTYs, FIFO-based synchronization, PTY window size initialization, relay to `/dev/tty`, capture to temp files (mode 0600), ANSI stripping, secret redaction, signal handling, inactivity timeout
- Updated shell hooks for zsh and bash (FIFO setup with `read -t 1` timeout, PTY redirect, crash recovery, polling wait, blacklist improvements)
- `--stdout-file`/`--stderr-file` flags on `redtrail capture` CLI command (capture reads, stores, deletes)
- `stderr_truncated` column in DB schema
- Rust-generated nanosecond timestamps (no `date +%s%N` dependency)
- Temp file header format with timestamps and truncation metadata

### Out of scope

- Fish shell support (Phase 1: "zsh first, then bash, then fish")
- Compression of large outputs (GUIDELINE mentions zlib — defer to Phase 1.3 follow-up)
- stdin capture (not in Phase 1 spec)
- Configurable capture toggle (can add later via `config.yaml`)
- SIGCHLD-based crash recovery in bash (accepted limitation)

### Intentional GUIDELINE deviations

- **Truncation vs compression:** GUIDELINE 1.3 says "compress with zlib, store as blob" for over-limit stdout. This spec truncates instead. Rationale: compression adds complexity (blob handling, decompression on read, FTS incompatibility) for minimal Phase 1 benefit. The 50KB limit captures the vast majority of useful output. Revisit when users report truncation as a pain point.

---

## Implementation Order

1. **`redtrail tee` binary** — PTY allocation, window size init, FIFO handshake, relay loop (`/dev/tty`), dual capture buffers, ANSI stripping, secret redaction, temp file output (mode 0600, with header), signal handling, inactivity timeout
2. **Schema change** — add `stderr_truncated` column
3. **`redtrail capture` updates** — `--stdout-file`/`--stderr-file` flags, read temp files (parse header for timestamps/truncation), pass content to `insert_command_redacted`, delete temp files after insert
4. **Shell hooks** — update `init.rs` with new zsh and bash hooks (FIFO setup with timeout, PTY redirect, crash recovery, polling wait, blacklist improvements, no shell-side timestamps)
5. **CLI plumbing** — register `tee` subcommand in `cli.rs`
6. **Tests** — PTY relay correctness, ANSI stripping, truncation per-stream, blacklist with path/env-prefix, FIFO timeout, crash recovery (zsh), temp file handoff with header parsing, temp file permissions, secret redaction on output, window size propagation
