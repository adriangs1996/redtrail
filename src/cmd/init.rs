use crate::error::Error;

const ZSH_HOOK: &str = r#"
# RedTrail shell integration — zsh
# eval "$(redtrail init zsh)"

export REDTRAIL_SESSION_ID="$(command redtrail session-id 2>/dev/null || echo "rt-$$-$(date +%s)")"

# Inline blacklist — no subprocess per command
__RT_BLACKLIST=":vim:nvim:nano:vi:ssh:scp:top:htop:btop:less:more:man:tmux:screen:"

__redtrail_preexec() {
    __REDTRAIL_CMD="$1"
    __REDTRAIL_CWD="$PWD"
    __REDTRAIL_TS_START="$(date +%s)"
    __RT_CAPTURE_ACTIVE=""

    # Blacklist check — extract binary, handle path-qualified and env-prefixed
    local cmd_str="$1"
    while :; do
        local word="${cmd_str%% *}"
        [[ "$word" == *=* ]] || break
        cmd_str="${cmd_str#"$word" }"
    done
    local binary="${cmd_str%% *}"
    binary="${binary##*/}"

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

        # Signal tee to flush and exit, then wait for it
        kill -USR1 "$__RT_TEE_PID" 2>/dev/null
        wait "$__RT_TEE_PID" 2>/dev/null
    fi

    [[ -z "$__REDTRAIL_CMD" ]] && return

    local ts_end
    ts_end="$(date +%s)"

    local -a capture_args=(
        --session-id "$REDTRAIL_SESSION_ID"
        --command "$__REDTRAIL_CMD"
        --cwd "$__REDTRAIL_CWD"
        --exit-code "$exit_code"
        --ts-start "$__REDTRAIL_TS_START"
        --ts-end "$ts_end"
        --shell zsh
        --hostname "${HOST:-$(hostname)}"
    )
    local out_file="/tmp/rt-out-$$" err_file="/tmp/rt-err-$$"
    [[ -f "$out_file" ]] && capture_args+=(--stdout-file "$out_file")
    [[ -f "$err_file" ]] && capture_args+=(--stderr-file "$err_file")

    command redtrail capture "${capture_args[@]}" 2>/dev/null &!

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
"#;

const BASH_HOOK: &str = r#"
# RedTrail shell integration — bash
# eval "$(redtrail init bash)"

export REDTRAIL_SESSION_ID="$(command redtrail session-id 2>/dev/null || echo "rt-$$-$(date +%s)")"

# Inline blacklist
__RT_BLACKLIST=":vim:nvim:nano:vi:ssh:scp:top:htop:btop:less:more:man:tmux:screen:"

__redtrail_preexec() {
    [ -n "$COMP_LINE" ] && return
    [ "$BASH_COMMAND" = "${PROMPT_COMMAND%%;*}" ] && return
    [ -n "$__RT_INSIDE_PRECMD" ] && return
    [ -n "$__RT_CAPTURE_ACTIVE" ] && return

    # Use history for full pipeline (BASH_COMMAND only has current simple command)
    __REDTRAIL_CMD="$(HISTTIMEFORMAT= history 1 | sed 's/^[ ]*[0-9]*[ ]*//')"
    __REDTRAIL_CWD="$PWD"
    __REDTRAIL_TS_START="$(date +%s)"

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

        # Signal tee to flush and exit, then wait for it
        kill -USR1 "$__RT_TEE_PID" 2>/dev/null
        wait "$__RT_TEE_PID" 2>/dev/null
    fi

    if [ -z "$__REDTRAIL_CMD" ]; then
        unset __RT_INSIDE_PRECMD
        return
    fi

    local ts_end
    ts_end="$(date +%s)"

    local capture_args=(
        --session-id "$REDTRAIL_SESSION_ID"
        --command "$__REDTRAIL_CMD"
        --cwd "$__REDTRAIL_CWD"
        --exit-code "$exit_code"
        --ts-start "$__REDTRAIL_TS_START"
        --ts-end "$ts_end"
        --shell bash
        --hostname "${HOSTNAME:-$(hostname)}"
    )
    local out_file="/tmp/rt-out-$$" err_file="/tmp/rt-err-$$"
    [[ -f "$out_file" ]] && capture_args+=(--stdout-file "$out_file")
    [[ -f "$err_file" ]] && capture_args+=(--stderr-file "$err_file")

    # capture runs SYNC in bash — reads and deletes temp files before returning
    command redtrail capture "${capture_args[@]}" 2>/dev/null

    unset __REDTRAIL_CMD __REDTRAIL_CWD __REDTRAIL_TS_START
    unset __RT_SAVE_OUT __RT_SAVE_ERR __RT_TEE_PID __RT_CAPTURE_ACTIVE
    unset __RT_INSIDE_PRECMD
}

trap '__redtrail_preexec' DEBUG
PROMPT_COMMAND="__redtrail_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
"#;

pub fn run(shell: &str) -> Result<(), Error> {
    match shell {
        "zsh" => {
            print!("{ZSH_HOOK}");
            Ok(())
        }
        "bash" => {
            print!("{BASH_HOOK}");
            Ok(())
        }
        other => {
            eprintln!("unsupported shell: {other}. Supported shells: zsh, bash");
            std::process::exit(1);
        }
    }
}
