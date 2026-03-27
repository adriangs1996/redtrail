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
3. **`redtrail capture`** reads the temp files via `--stdout-file`/`--stderr-file` flags and stores them with the command row. Single writer, single reader, no race.

### Why this works

- **No command rewriting** — the shell executes the user's command verbatim
- **No eval** — no quoting/expansion issues
- **No history pollution** — hooks are invisible
- **Multi-line commands work** — we don't parse or transform the command
- **One architecture** — identical approach for zsh and bash
- **TTY truly preserved** — the shell redirects stdout to the PTY slave fd directly, so `isatty()` returns true for the command's stdout/stderr
- **No race condition** — `redtrail tee` writes to temp files, `redtrail capture` reads them sequentially

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                         USER'S SHELL                                 │
│                                                                      │
│  preexec:                                                            │
│    mkfifo /tmp/rt-ctl-$$                                             │
│    redtrail tee --session $SID --ts $TS --ctl /tmp/rt-ctl-$$ &      │
│    read pty_out pty_err < /tmp/rt-ctl-$$   (blocks until PTY ready)  │
│    rm -f /tmp/rt-ctl-$$                                              │
│    exec {RT_SAVE_OUT}>&1 {RT_SAVE_ERR}>&2                           │
│    exec 1>$pty_out 2>$pty_err                                        │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │               USER'S COMMAND RUNS                              │  │
│  │                                                                │  │
│  │  stdout fd ──→ /dev/pts/X (PTY slave) ──→ isatty() == true    │  │
│  │                                                                │  │
│  │  redtrail tee reads PTY master:                                │  │
│  │    ├──→ writes to /dev/tty (real terminal — user sees output)  │  │
│  │    └──→ accumulates in capture buffer                          │  │
│  │                                                                │  │
│  │  stderr fd ──→ /dev/pts/Y (PTY slave) ──→ isatty() == true    │  │
│  │                                                                │  │
│  │  redtrail tee reads PTY master:                                │  │
│  │    ├──→ writes to /dev/tty (real terminal)                     │  │
│  │    └──→ accumulates in capture buffer                          │  │
│  └────────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  precmd:                                                             │
│    exec 1>&${RT_SAVE_OUT} 2>&${RT_SAVE_ERR}                         │
│    exec {RT_SAVE_OUT}>&- {RT_SAVE_ERR}>&-                            │
│    (PTY slaves closed → master EOF → tee writes temp files, exits)   │
│    wait $__RT_TEE_PID                                                │
│    redtrail capture ... --stdout-file /tmp/rt-out-$$ \               │
│                         --stderr-file /tmp/rt-err-$$                 │
│    rm -f /tmp/rt-out-$$ /tmp/rt-err-$$                               │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

---

## `redtrail tee` Binary Design

### Invocation

```
redtrail tee --session <session-id> --ts-start <nanosecond-ts> --ctl-fifo <path> [--max-bytes 51200]
```

Single process handles both stdout and stderr PTYs.

### Behavior

1. **Allocate two PTY pairs** — one for stdout, one for stderr. Each has a master (read side) and slave (write side).
2. **Write slave paths to FIFO** — write `"/dev/pts/X /dev/pts/Y"` to `--ctl-fifo`. This unblocks the shell, which is waiting on `read < fifo`.
3. **Relay loop** — poll both PTY masters. For each chunk:
   - Write to `/dev/tty` (the real controlling terminal, bypassing any fd redirection)
   - Append to the corresponding capture buffer (stdout or stderr), bounded by `--max-bytes`
4. **On EOF** (both masters see EOF after shell closes the slave fds):
   - Strip ANSI escape sequences from captured buffers
   - Run secret redaction on the cleaned buffers
   - Write stdout capture to `/tmp/rt-out-<shell-pid>`, stderr to `/tmp/rt-err-<shell-pid>`
   - Exit with code 0
5. **Signal handling:**
   - **SIGWINCH** — forward terminal resize to both PTY slaves
   - **SIGCHLD** — the shell installs a trap so that if `redtrail tee` dies, fds are restored immediately (see Crash Recovery)
   - **SIGINT/SIGTERM** — write whatever is in the buffer to temp files before exiting, so partial output is still captured

### Capture Buffer

- Default max: 50KB (`MAX_STDOUT_BYTES` already defined in `capture.rs`)
- Separate buffers for stdout and stderr, each independently bounded
- When a buffer exceeds max, stop accumulating but keep relaying to `/dev/tty`
- Record truncation status per-stream in a sidecar: temp file header or separate flag file

### PTY Allocation

Use the `nix` crate for POSIX PTY operations:

- `posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY)` — create master
- `grantpt()` / `unlockpt()` — prepare slave
- `ptsname_r()` — get slave path (use `_r` variant for thread safety)

The shell opens the slave path directly via `exec 1>/dev/pts/X`. The command inherits this fd. `isatty(1)` returns `true` because the fd points to a real PTY device.

`redtrail tee` holds the master fds and reads output from them.

### ANSI Stripping

Use the `strip-ansi-escapes` crate — it handles CSI sequences, OSC sequences, DCS sequences, cursor save/restore, and title-setting escapes. Do not use a regex; it will miss non-CSI escapes.

### Relay Target: `/dev/tty`

`redtrail tee` writes relayed output to `/dev/tty`, not to its inherited stdout. This is important because:

- The process is backgrounded (`&`) — its stdout may be redirected or detached
- `/dev/tty` always refers to the controlling terminal of the session
- This ensures the user sees output regardless of how the tee process was spawned

---

## DB Write Strategy: Temp File Handoff

No race condition. No retry logic. No concurrent DB writers.

1. `redtrail tee` writes captured output to temp files on EOF:
   - `/tmp/rt-out-<shell-pid>` — stdout capture (ANSI-stripped, secret-redacted)
   - `/tmp/rt-err-<shell-pid>` — stderr capture (ANSI-stripped, secret-redacted)
   - Files include a header line: `truncated:true` or `truncated:false`
2. Shell hook calls `wait $__RT_TEE_PID` to ensure tee has finished writing
3. Shell hook calls `redtrail capture --stdout-file /tmp/rt-out-$$ --stderr-file /tmp/rt-err-$$ ...`
4. `redtrail capture` reads the files, inserts the complete command row (metadata + output) in one operation
5. Shell hook removes temp files

This is sequential: tee finishes → capture reads → done. One DB writer, one transaction.

### Temp File Format

```
truncated:false
<captured output content>
```

First line is metadata. Rest is the captured content. If the file doesn't exist or is empty, that stream had no output — capture stores NULL.

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
    __REDTRAIL_TS_START="$(date +%s%N)"
    __RT_CAPTURE_ACTIVE=""

    # Blacklist check — extract binary, handle path-qualified and env-prefixed commands
    local cmd_str="$1"
    # Strip leading env assignments: FOO=bar BAZ=qux cmd ...
    while [[ "$cmd_str" == *=* ]]; do
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
        --ts-start "$__REDTRAIL_TS_START" \
        --shell-pid "$$" \
        --ctl-fifo "$ctl_fifo" \
        2>/dev/null &
    __RT_TEE_PID=$!

    local pty_out pty_err
    read pty_out pty_err < "$ctl_fifo" || { rm -f "$ctl_fifo"; return; }
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

        # Wait for tee to finish writing temp files
        wait "$__RT_TEE_PID" 2>/dev/null
    fi

    [[ -z "$__REDTRAIL_CMD" ]] && return

    local ts_end
    ts_end="$(date +%s%N)"

    local stdout_arg="" stderr_arg=""
    local out_file="/tmp/rt-out-$$" err_file="/tmp/rt-err-$$"
    [[ -f "$out_file" ]] && stdout_arg="--stdout-file $out_file"
    [[ -f "$err_file" ]] && stderr_arg="--stderr-file $err_file"

    command redtrail capture \
        --session-id "$REDTRAIL_SESSION_ID" \
        --command "$__REDTRAIL_CMD" \
        --cwd "$__REDTRAIL_CWD" \
        --exit-code "$exit_code" \
        --ts-start "$__REDTRAIL_TS_START" \
        --ts-end "$ts_end" \
        --shell zsh \
        --hostname "${HOST:-$(hostname)}" \
        $stdout_arg $stderr_arg \
        2>/dev/null

    rm -f "$out_file" "$err_file" 2>/dev/null

    unset __REDTRAIL_CMD __REDTRAIL_CWD __REDTRAIL_TS_START
    unset __RT_SAVE_OUT __RT_SAVE_ERR __RT_TEE_PID __RT_CAPTURE_ACTIVE
}

# Crash recovery: if tee dies, restore fds immediately
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
    __REDTRAIL_TS_START="$(date +%s%N)"

    # Blacklist check
    local cmd_str="$__REDTRAIL_CMD"
    while [[ "$cmd_str" == *=* ]]; do
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
        --ts-start "$__REDTRAIL_TS_START" \
        --shell-pid "$$" \
        --ctl-fifo "$ctl_fifo" \
        2>/dev/null &
    __RT_TEE_PID=$!

    local pty_out pty_err
    read pty_out pty_err < "$ctl_fifo" || { rm -f "$ctl_fifo"; return; }
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

        wait "$__RT_TEE_PID" 2>/dev/null
    fi

    if [ -z "$__REDTRAIL_CMD" ]; then
        unset __RT_INSIDE_PRECMD
        return
    fi

    local ts_end
    ts_end="$(date +%s%N)"

    local stdout_arg="" stderr_arg=""
    local out_file="/tmp/rt-out-$$" err_file="/tmp/rt-err-$$"
    [[ -f "$out_file" ]] && stdout_arg="--stdout-file $out_file"
    [[ -f "$err_file" ]] && stderr_arg="--stderr-file $err_file"

    command redtrail capture \
        --session-id "$REDTRAIL_SESSION_ID" \
        --command "$__REDTRAIL_CMD" \
        --cwd "$__REDTRAIL_CWD" \
        --exit-code "$exit_code" \
        --ts-start "$__REDTRAIL_TS_START" \
        --ts-end "$ts_end" \
        --shell bash \
        --hostname "${HOSTNAME:-$(hostname)}" \
        $stdout_arg $stderr_arg \
        2>/dev/null &

    rm -f "$out_file" "$err_file" 2>/dev/null

    unset __REDTRAIL_CMD __REDTRAIL_CWD __REDTRAIL_TS_START
    unset __RT_SAVE_OUT __RT_SAVE_ERR __RT_TEE_PID __RT_CAPTURE_ACTIVE
    unset __RT_INSIDE_PRECMD
}

trap '__redtrail_preexec' DEBUG
PROMPT_COMMAND="__redtrail_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
```

### Bash-specific notes

**Full command capture:** Bash's `$BASH_COMMAND` only contains the current simple command in a pipeline. `cat foo | grep bar` gives `cat foo` on first DEBUG trap fire. We use `history 1` instead to get the full command line as the user typed it. The `HISTTIMEFORMAT=` prefix strips timestamp formatting.

**Compound command guard:** The `__RT_CAPTURE_ACTIVE` flag prevents the DEBUG trap from re-entering on subsequent simple commands within a compound command (`cmd1 && cmd2`). The combined stdout/stderr of the entire compound command is captured as a single blob.

**No SIGCHLD trap in bash:** Bash's trap mechanism is less flexible than zsh's TRAPCHLD. If `redtrail tee` crashes mid-command in bash, output goes dark until precmd fires. This is an accepted limitation — the precmd always restores fds, so the shell recovers. Long-running commands are the risk window. A future improvement could poll `kill -0` on a short timer.

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

### bash

No equivalent SIGCHLD handler that works reliably during command execution. Accepted limitation — precmd always restores fds. The window of risk is the duration of the current command.

---

## Blacklist Handling

The inline blacklist check handles:

- **Simple commands:** `vim file` — extracts `vim`
- **Path-qualified:** `/usr/bin/vim file` — extracts `vim` via `${binary##*/}`
- **Env-prefixed:** `EDITOR=vim FOO=bar vim file` — strips `VAR=val` prefixes before extracting binary
- **`command`/`builtin` prefixes:** Not handled — `command vim` extracts `command`. Acceptable: `command vim` is rare and the user would see normal behavior (just no stdout capture for that invocation).

---

## Signal Handling

| Signal | Behavior |
|--------|----------|
| SIGWINCH | `redtrail tee` forwards to PTY slaves via `ioctl(TIOCSWINSZ)` so commands handle terminal resize |
| SIGINT (Ctrl+C) | Delivered to the foreground process group. `redtrail tee` catches it, writes partial buffer to temp files, exits. Shell restores fds in precmd. |
| SIGTSTP (Ctrl+Z) | Foreground command is suspended. Shell's precmd fires, restores fds, closes slaves. Tee sees EOF, writes what it has. If the user later `fg`s the job, it writes to the original (restored) stdout — no tee in the path. Partial capture only. Accepted limitation. |
| SIGTERM/SIGHUP | `redtrail tee` writes buffer to temp files before exiting |

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

Change `timestamp_start` and `timestamp_end` from seconds to nanoseconds. The shell hooks now use `date +%s%N`. Existing data (seconds) will look like very old timestamps — this is acceptable since we're pre-release. No migration needed.

**Important:** The `(session_id, timestamp_start)` pair is no longer used for DB lookups (the temp file approach eliminated that need), so the precision change is purely for accuracy, not for uniqueness guarantees.

---

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| `redtrail tee` binary missing | `mkfifo` succeeds but `read` on FIFO hangs | Timeout on FIFO read (1 second), skip capture on timeout |
| `redtrail tee` crashes mid-command | Output goes dark until SIGCHLD (zsh) or precmd (bash) | TRAPCHLD restores fds in zsh; precmd always restores in both |
| PTY allocation fails | `redtrail tee` writes error to FIFO, shell skips capture | Fall back to no-capture, log warning |
| FIFO creation fails (`/tmp` full) | `mkfifo` returns error, preexec returns early | No capture for that command, no user impact |
| DB write fails in capture | Command metadata still stored, output field is NULL | Log error, user unaffected |
| Temp file write fails | `redtrail capture` sees no file, stores NULL for output | No capture for that command |
| Very large output (>50KB) | Buffer capped per-stream, rest relayed but not stored | `stdout_truncated`/`stderr_truncated = true` |
| Binary output | Stored as-is up to limit, not useful for extraction | Future: detect binary content and skip storage |
| `redtrail tee` hangs (no EOF) | Blocks the prompt if `wait` is used | FIFO read timeout; inactivity timeout in tee (5min) |

**Key invariant:** If any part of the capture pipeline fails, the user's terminal behavior is unchanged. Every failure path restores fds and continues. Capture is best-effort.

---

## New Dependencies

- `nix` crate — POSIX PTY operations (`posix_openpt`, `grantpt`, `unlockpt`, `ptsname_r`), signal handling, `ioctl` for SIGWINCH forwarding
- `strip-ansi-escapes` crate — handles CSI, OSC, DCS sequences and all terminal escape codes

---

## Performance Budget

- **preexec overhead:** `mkfifo` + fork `redtrail tee` + block on FIFO read + two `exec` redirects. Target: <25ms total. The FIFO synchronization adds ~2-5ms over the old approach.
- **precmd overhead:** Two `exec` fd restores + `wait` for tee + one `redtrail capture` spawn + two `rm`. Target: <10ms added over current. The `wait` should be near-instant since tee exits on EOF.
- **`redtrail tee` relay latency:** PTY read → `/dev/tty` write should add <1ms per chunk.
- **`redtrail tee` process lifetime:** Lives for the duration of the command. Uses <2MB memory (two capture buffers + two PTYs). Exits on EOF.
- **Total per-command budget:** <50ms overhead (preexec + precmd combined), consistent with CLAUDE.md mandate.

Single `redtrail tee` process for both streams avoids the cost of two Rust binary spawns.

---

## Scope

### In scope

- `redtrail tee` binary — single process, two PTYs, FIFO-based synchronization, relay to `/dev/tty`, capture to temp files
- Updated shell hooks for zsh and bash (FIFO-based PTY setup, crash recovery, nanosecond timestamps)
- `--stdout-file`/`--stderr-file` flags on `redtrail capture` CLI command
- `stderr_truncated` column in DB schema
- ANSI escape stripping via `strip-ansi-escapes` crate
- Blacklist bypass with env-prefix and path-qualified binary handling
- Secret redaction on captured output (reuse existing `redact_secrets`)
- Inactivity timeout in `redtrail tee` (5 minutes)
- SIGWINCH forwarding for terminal resize

### Out of scope

- Fish shell support (Phase 1: "zsh first, then bash, then fish")
- Compression of large outputs (GUIDELINE mentions zlib — defer to Phase 1.3 follow-up, document as intentional deviation: truncation is simpler and sufficient for now)
- stdin capture (not in Phase 1 spec)
- Configurable capture toggle (can add later via `config.yaml`)
- SIGCHLD-based crash recovery in bash (accepted limitation)

### Intentional GUIDELINE deviations

- **Truncation vs compression:** GUIDELINE 1.3 says "compress with zlib, store as blob" for over-limit stdout. This spec truncates instead. Rationale: compression adds complexity (blob handling, decompression on read, FTS incompatibility) for minimal Phase 1 benefit. The 50KB limit captures the vast majority of useful output. Revisit when users report truncation as a pain point.

---

## Implementation Order

1. **`redtrail tee` binary** — PTY allocation, FIFO handshake, relay loop (`/dev/tty`), dual capture buffers, ANSI stripping, secret redaction, temp file output, signal handling, inactivity timeout
2. **Schema change** — add `stderr_truncated` column
3. **`redtrail capture` updates** — `--stdout-file`/`--stderr-file` flags, read temp files, pass content to `insert_command_redacted`
4. **Shell hooks** — update `init.rs` with new zsh and bash hooks (FIFO setup, PTY redirect, crash recovery, nanosecond timestamps, blacklist improvements)
5. **CLI plumbing** — register `tee` subcommand in `cli.rs`
6. **Tests** — PTY relay correctness, ANSI stripping, truncation per-stream, blacklist with path/env-prefix, FIFO timeout, crash recovery (zsh), temp file handoff, secret redaction on output
