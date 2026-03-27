# Stdout/Stderr Capture via `redtrail tee`

> Phase 1 — Silent Capture
> Date: 2026-03-27

---

## Problem

RedTrail captures command metadata (command string, exit code, cwd, timing) but not stdout/stderr. The DB schema supports it, the `NewCommand` struct accepts it, but the shell hooks never pass output data. Without stdout/stderr, full-text search across output is empty, extraction in Phase 2 has nothing to parse, and error-resolution mapping in Phase 3 has no error messages to work with.

Capturing stdout/stderr from a shell hook is hard because:

- **eval-based wrapping** breaks quoting semantics and double-expands variables
- **Pipe-based tee** kills TTY detection — commands lose colors, pagers, progress bars
- **Command rewriting** (e.g., `accept-line` in zsh) pollutes shell history in bash, can't handle multi-line commands in bash (`bind -x` fires per-line), and requires maintaining two different architectures
- **Process substitution race conditions** — tee may not flush before precmd reads the file

---

## Solution: fd-dup + `redtrail tee --pty`

A single architecture for both zsh and bash:

1. **preexec/DEBUG trap:** Save original stdout/stderr file descriptors, then redirect them through a `redtrail tee --pty` process
2. **precmd/PROMPT_COMMAND:** Restore original fds (this closes `redtrail tee`'s input, triggering flush and DB write)
3. **`redtrail tee`** is a PTY-aware binary that preserves terminal behavior while capturing output

### Why this works

- **No command rewriting** — the shell executes the user's command verbatim
- **No eval** — no quoting/expansion issues
- **No history pollution** — hooks are invisible
- **Multi-line commands work** — we don't parse or transform the command
- **One architecture** — identical approach for zsh and bash
- **TTY preserved** — `redtrail tee --pty` allocates a pseudo-terminal so upstream commands see `isatty() == true`

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        USER'S SHELL                         │
│                                                             │
│  preexec:                                                   │
│    exec {RT_SAVE_OUT}>&1                                    │
│    exec {RT_SAVE_ERR}>&2                                    │
│    exec 1> >(redtrail tee --pty --session $SID --fd out)    │
│    exec 2> >(redtrail tee --pty --session $SID --fd err)    │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              USER'S COMMAND RUNS                      │  │
│  │  stdout ──→ PTY ──→ redtrail tee ──→ real terminal   │  │
│  │                          └──→ capture buffer          │  │
│  │  stderr ──→ PTY ──→ redtrail tee ──→ real terminal   │  │
│  │                          └──→ capture buffer          │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                             │
│  precmd:                                                    │
│    exec 1>&${RT_SAVE_OUT} {RT_SAVE_OUT}>&-                  │
│    exec 2>&${RT_SAVE_ERR} {RT_SAVE_ERR}>&-                  │
│    (redtrail tee sees EOF, flushes to DB, exits)            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## `redtrail tee` Binary Design

### Invocation

```
redtrail tee --pty --session <session-id> --fd <out|err> [--max-bytes 51200]
```

### Behavior

1. **Allocate PTY pair** — create a pseudo-terminal (master/slave). The slave side becomes stdin for the tee process. Commands writing to this fd see `isatty() == true`.
2. **Read loop** — read from PTY master. For each chunk:
   - Write to the real output fd (stdout or stderr, inherited from the shell)
   - Append to an in-memory capture buffer (bounded by `--max-bytes`, default 50KB)
3. **On EOF** (shell restores fds, closing our input):
   - Run secret redaction on the captured buffer
   - Write the captured output to the DB via `insert_command_redacted` (or a new endpoint that updates an existing command row)
   - Exit with code 0
4. **Signal forwarding** — forward SIGWINCH (terminal resize) from the real terminal to the PTY slave so commands handle resize correctly.

### Capture Buffer

- Default max: 50KB (`MAX_STDOUT_BYTES` already defined in `capture.rs`)
- When buffer exceeds max, stop accumulating but keep relaying to real stdout
- Set `stdout_truncated = true` on the DB record
- Strip ANSI escape sequences from the stored capture (raw terminal escapes are noise for extraction)

### PTY Allocation

Use the `nix` crate (or `rustix`) for POSIX PTY operations:
- `posix_openpt()` — create master
- `grantpt()` / `unlockpt()` — prepare slave
- `ptsname()` — get slave path

The PTY slave fd is what process substitution connects to. The master fd is what `redtrail tee` reads from.

### DB Write Strategy

The command row is inserted in two phases:

1. **preexec time** — `redtrail capture` inserts the row with stdout/stderr as NULL (this already happens today)
2. **precmd time** — `redtrail tee` updates the existing row with captured output

This requires a new DB function:

```rust
pub fn update_command_output(
    conn: &Connection,
    session_id: &str,
    ts_start: i64,
    fd: &str,         // "stdout" or "stderr"
    output: &str,
    truncated: bool,
) -> Result<(), Error>
```

Lookup by `(session_id, ts_start)` since `redtrail tee` doesn't know the command row ID. The combination is unique within a session.

Alternatively: `redtrail tee` receives the command ID directly. But the ID is generated inside `redtrail capture`, and passing it back to the shell hook adds complexity. The `(session_id, ts_start)` lookup is simpler.

### ANSI Stripping

Store cleaned output for extraction/search. The raw terminal output contains cursor movement, color codes, and other escape sequences that are useless for text analysis.

Use the `strip-ansi-escapes` crate or a simple regex: `\x1b\[[0-9;]*[a-zA-Z]`.

---

## Shell Hook Changes

### zsh

```zsh
# RedTrail shell integration — zsh
# eval "$(redtrail init zsh)"

export REDTRAIL_SESSION_ID="$(command redtrail session-id 2>/dev/null || echo "rt-$$-$(date +%s)")"

__redtrail_preexec() {
    export __REDTRAIL_CMD="$1"
    export __REDTRAIL_CWD="$PWD"
    export __REDTRAIL_TS_START="$(date +%s)"

    # Capture stdout
    exec {__RT_SAVE_OUT}>&1
    exec 1> >(command redtrail tee --pty \
        --session "$REDTRAIL_SESSION_ID" \
        --ts-start "$__REDTRAIL_TS_START" \
        --fd out 2>/dev/null >&${__RT_SAVE_OUT})

    # Capture stderr
    exec {__RT_SAVE_ERR}>&2
    exec 2> >(command redtrail tee --pty \
        --session "$REDTRAIL_SESSION_ID" \
        --ts-start "$__REDTRAIL_TS_START" \
        --fd err 2>/dev/null >&${__RT_SAVE_ERR})
}

__redtrail_precmd() {
    local exit_code=$?

    # Restore fds (triggers EOF → redtrail tee flushes and exits)
    [[ -n "$__RT_SAVE_OUT" ]] && exec 1>&${__RT_SAVE_OUT} {__RT_SAVE_OUT}>&-
    [[ -n "$__RT_SAVE_ERR" ]] && exec 2>&${__RT_SAVE_ERR} {__RT_SAVE_ERR}>&-

    [ -z "$__REDTRAIL_CMD" ] && return

    local ts_end
    ts_end="$(date +%s)"

    # Record command metadata (stdout/stderr already written by tee)
    command redtrail capture \
        --session-id "$REDTRAIL_SESSION_ID" \
        --command "$__REDTRAIL_CMD" \
        --cwd "$__REDTRAIL_CWD" \
        --exit-code "$exit_code" \
        --ts-start "$__REDTRAIL_TS_START" \
        --ts-end "$ts_end" \
        --shell zsh \
        --hostname "${HOST:-$(hostname)}" \
        2>/dev/null &!

    unset __REDTRAIL_CMD __REDTRAIL_CWD __REDTRAIL_TS_START
    unset __RT_SAVE_OUT __RT_SAVE_ERR
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

__redtrail_preexec() {
    [ -n "$COMP_LINE" ] && return
    [ "$BASH_COMMAND" = "${PROMPT_COMMAND%%;*}" ] && return
    [ -n "$__RT_INSIDE_PRECMD" ] && return

    export __REDTRAIL_CMD="$BASH_COMMAND"
    export __REDTRAIL_CWD="$PWD"
    export __REDTRAIL_TS_START="$(date +%s)"

    # Capture stdout
    exec {__RT_SAVE_OUT}>&1
    exec 1> >(command redtrail tee --pty \
        --session "$REDTRAIL_SESSION_ID" \
        --ts-start "$__REDTRAIL_TS_START" \
        --fd out 2>/dev/null >&${__RT_SAVE_OUT})

    # Capture stderr
    exec {__RT_SAVE_ERR}>&2
    exec 2> >(command redtrail tee --pty \
        --session "$REDTRAIL_SESSION_ID" \
        --ts-start "$__REDTRAIL_TS_START" \
        --fd err 2>/dev/null >&${__RT_SAVE_ERR})
}

__redtrail_precmd() {
    local exit_code=$?
    __RT_INSIDE_PRECMD=1

    # Restore fds
    [[ -n "$__RT_SAVE_OUT" ]] && exec 1>&${__RT_SAVE_OUT} {__RT_SAVE_OUT}>&-
    [[ -n "$__RT_SAVE_ERR" ]] && exec 2>&${__RT_SAVE_ERR} {__RT_SAVE_ERR}>&-

    [ -z "$__REDTRAIL_CMD" ] && { unset __RT_INSIDE_PRECMD; return; }

    local ts_end
    ts_end="$(date +%s)"

    command redtrail capture \
        --session-id "$REDTRAIL_SESSION_ID" \
        --command "$__REDTRAIL_CMD" \
        --cwd "$__REDTRAIL_CWD" \
        --exit-code "$exit_code" \
        --ts-start "$__REDTRAIL_TS_START" \
        --ts-end "$ts_end" \
        --shell bash \
        --hostname "${HOSTNAME:-$(hostname)}" \
        2>/dev/null &

    unset __REDTRAIL_CMD __REDTRAIL_CWD __REDTRAIL_TS_START
    unset __RT_SAVE_OUT __RT_SAVE_ERR
    unset __RT_INSIDE_PRECMD
}

trap '__redtrail_preexec' DEBUG
PROMPT_COMMAND="__redtrail_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
```

### bash DEBUG trap edge case

The `DEBUG` trap fires before **every simple command**, not once per compound command. For `cmd1 && cmd2`, the trap fires twice. The `__RT_INSIDE_PRECMD` guard prevents re-entry during precmd, but we also need a guard to prevent re-entry during a compound command:

```bash
__redtrail_preexec() {
    # ... existing guards ...
    [ -n "$__RT_CAPTURE_ACTIVE" ] && return  # already capturing this compound command
    __RT_CAPTURE_ACTIVE=1
    # ... fd setup ...
}

__redtrail_precmd() {
    # ... fd restore ...
    unset __RT_CAPTURE_ACTIVE
    # ... capture call ...
}
```

This means for `cmd1 && cmd2`, we capture the combined stdout of both commands as a single blob. That's correct — the user sees them as one logical execution.

---

## Blacklist Handling

For blacklisted commands (vim, ssh, top, etc.), skip the fd redirection entirely:

```zsh
__redtrail_preexec() {
    # ...
    local binary="${1%% *}"
    if command redtrail is-blacklisted "$binary" 2>/dev/null; then
        __RT_SKIP_CAPTURE=1
        return
    fi
    # ... fd setup ...
}
```

Or to avoid a subprocess call on every command (performance), inline the blacklist check:

```zsh
__RT_BLACKLIST="vim:nvim:nano:vi:ssh:scp:top:htop:btop:less:more:man:tmux:screen"

__redtrail_preexec() {
    local binary="${1%% *}"
    if [[ ":$__RT_BLACKLIST:" == *":$binary:"* ]]; then
        __RT_SKIP_CAPTURE=1
        return
    fi
    # ... fd setup ...
}
```

This keeps the blacklist check entirely in-shell with zero subprocess overhead.

---

## Race Condition: tee flush vs capture write

There's a timing concern: `redtrail capture` (called in precmd) inserts the command row, while `redtrail tee` (triggered by fd close in precmd) updates it with output. Both happen near-simultaneously.

Ordering guarantee: In precmd, we restore fds **first** (triggering tee), then call `redtrail capture`. But `redtrail tee` runs in a process substitution — it's async. It might not finish its DB write before `redtrail capture` inserts the row. Or `redtrail capture` might finish before `redtrail tee` starts its write.

**Solution: `redtrail tee` does a retry/upsert.**

`redtrail tee` attempts to update the command row by `(session_id, ts_start)`. If the row doesn't exist yet (capture hasn't run), tee waits briefly (10ms, then 50ms) and retries. Max 3 retries. If it still doesn't exist, tee writes to a spool file that `redtrail capture` checks on next invocation.

Alternatively, flip the order: `redtrail tee` inserts the full row (it has session_id, ts_start, fd content) and `redtrail capture` updates it with metadata. But this changes the current flow significantly.

**Recommended approach:** `redtrail capture` runs first (sync, in precmd), then `redtrail tee` updates with output (async, triggered by fd close). The capture insert is fast. By the time tee has finished processing its buffer and runs its update, the row exists. In practice, the race is unlikely — but the retry handles it if it happens.

---

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| `redtrail tee` binary missing | fd redirect fails silently, command output goes to original fd | `2>/dev/null` on exec, user sees normal behavior |
| `redtrail tee` crashes mid-command | Output stops reaching terminal | PTY master EOF propagates, shell restores fds in precmd |
| PTY allocation fails | `redtrail tee` falls back to plain pipe mode (no TTY) | Log warning, colors degrade for that command |
| DB write fails in tee | Output was still relayed to terminal, just not stored | Log error, user unaffected |
| Very large output (>50KB) | Buffer capped, rest relayed but not stored | `stdout_truncated = true` |
| Binary output (images, compiled objects) | Stored as-is up to limit, not useful for extraction | Could detect and skip binary content |

**Key invariant:** If `redtrail tee` fails in any way, the user's terminal behavior is unchanged. Capture is best-effort. The shell hooks are defensive — they always restore fds in precmd regardless of what happened.

---

## New Dependencies

- `nix` crate (for PTY operations) — already common in Rust CLI tools, well-maintained
- `strip-ansi-escapes` crate (for cleaning stored output) — small, focused

---

## Performance Budget

- **preexec overhead:** Two `exec` fd redirects + two `redtrail tee` process spawns. Target: <20ms total.
- **precmd overhead:** Two `exec` fd restores + one `redtrail capture` spawn (already exists). The fd restores are instant. Target: <5ms added over current.
- **`redtrail tee` relay latency:** PTY read → stdout write should add <1ms per chunk. The user must not perceive any delay in command output appearing.
- **`redtrail tee` process lifetime:** Lives for the duration of the command. Uses <1MB memory (capture buffer + PTY overhead). Exits on EOF.

The two `redtrail tee` spawns in preexec are the biggest cost. If this exceeds budget, we can optimize by:
1. Spawning a single `redtrail tee` that handles both stdout and stderr on separate fds
2. Pre-forking a long-lived tee process per session that accepts commands over a control channel

---

## Scope

### In scope
- `redtrail tee` binary with PTY support
- Updated shell hooks for zsh and bash
- DB update function for stdout/stderr on existing command rows
- ANSI escape stripping for stored output
- Blacklist bypass (skip capture for interactive commands)
- Secret redaction on captured output (already exists, reuse `redact_secrets`)

### Out of scope
- Fish shell support (Phase 1 says "zsh first, then bash, then fish")
- Compression of large outputs (GUIDELINE mentions zlib — defer until needed)
- stdin capture (not in Phase 1 spec)
- Configurable capture toggle (can add later via `config.yaml`)

---

## Implementation Order

1. `redtrail tee` binary — PTY allocation, read/relay loop, ANSI stripping, buffer management
2. DB update function — `update_command_output(session_id, ts_start, fd, output, truncated)`
3. Secret redaction integration — run `redact_secrets` on captured output before DB write
4. Shell hooks — update `init.rs` with new zsh and bash hooks
5. CLI plumbing — register `tee` subcommand in `cli.rs`
6. Tests — PTY relay correctness, ANSI stripping, truncation, blacklist bypass, race condition handling
