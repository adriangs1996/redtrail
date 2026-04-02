use crate::error::Error;

const ZSH_HOOK: &str = r#"
# RedTrail shell integration — zsh
# eval "$(redtrail init zsh)"

export REDTRAIL_SESSION_ID="$(command redtrail session-id 2>/dev/null || echo "rt-$$-$(date +%s)")"

# Inline blacklist — no subprocess per command
__RT_BLACKLIST=":vim:nvim:nano:vi:ssh:scp:top:htop:btop:less:more:man:tmux:screen:"

__redtrail_preexec() {
    setopt local_options no_monitor

    # Escape hatch: REDTRAIL_SKIP=1 cmd skips capture entirely
    if [[ "$1" == REDTRAIL_SKIP=1\ * || "$1" == *\ REDTRAIL_SKIP=1\ * ]]; then
        return
    fi

    __REDTRAIL_CWD="$PWD"
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

    # Resolve aliases so e.g. alias v=nvim is caught by the blacklist
    if (( ${+aliases[$binary]} )); then
        local resolved="${aliases[$binary]%% *}"
        resolved="${resolved##*/}"
        if [[ "$__RT_BLACKLIST" == *":$resolved:"* ]]; then
            return
        fi
    fi

    # Register command with DB — get back a command ID for tee and finish
    __REDTRAIL_CMD_ID=$(command redtrail capture start \
        --session-id "$REDTRAIL_SESSION_ID" \
        --command "$1" \
        --cwd "$PWD" \
        --shell zsh \
        --hostname "${HOST:-$(hostname)}" \
        2>/dev/null)
    [[ -z "$__REDTRAIL_CMD_ID" ]] && return

    # Set up capture via redtrail tee
    local ctl_fifo="/tmp/rt-ctl-$$"
    mkfifo "$ctl_fifo" 2>/dev/null || return

    command redtrail tee \
        --command-id "$__REDTRAIL_CMD_ID" \
        --shell-pid "$$" \
        --ctl-fifo "$ctl_fifo" \
        2>/dev/null &
    __RT_TEE_PID=$!
    disown

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
    setopt local_options no_monitor

    # Restore fds — always, even if capture setup partially failed
    if [[ -n "$__RT_CAPTURE_ACTIVE" ]]; then
        exec 1>&${__RT_SAVE_OUT} 2>&${__RT_SAVE_ERR}
        exec {__RT_SAVE_OUT}>&- {__RT_SAVE_ERR}>&-

        # Signal tee to flush and exit, then wait with timeout (poll — disowned jobs can't use wait)
        kill -USR1 "$__RT_TEE_PID" 2>/dev/null
        local __rt_w=0
        while kill -0 "$__RT_TEE_PID" 2>/dev/null; do
            sleep 0.01
            (( __rt_w++ ))
            (( __rt_w >= 200 )) && { kill -9 "$__RT_TEE_PID" 2>/dev/null; break; }
        done
    fi

    [[ -z "$__REDTRAIL_CMD_ID" ]] && return

    # Finalize the command record — backgrounded to not block the prompt
    command redtrail capture finish \
        --command-id "$__REDTRAIL_CMD_ID" \
        --exit-code "$exit_code" \
        --cwd "$__REDTRAIL_CWD" \
        2>/dev/null &!

    unset __REDTRAIL_CMD_ID __REDTRAIL_CWD
    unset __RT_SAVE_OUT __RT_SAVE_ERR __RT_TEE_PID __RT_CAPTURE_ACTIVE
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
    local saved_exit=$?
    [ -n "$COMP_LINE" ] && return $saved_exit
    [ "$BASH_COMMAND" = "${PROMPT_COMMAND%%;*}" ] && return $saved_exit
    [ -n "$__RT_INSIDE_PRECMD" ] && return $saved_exit
    [ -n "$__RT_CAPTURE_ACTIVE" ] && return $saved_exit

    # Use history for full pipeline (BASH_COMMAND only has current simple command)
    local __rt_cmd
    __rt_cmd="$(HISTTIMEFORMAT= history 1 | sed 's/^[ ]*[0-9]*[ ]*//')"
    __REDTRAIL_CWD="$PWD"

    # Escape hatch: REDTRAIL_SKIP=1 cmd skips capture entirely
    if [[ "$__rt_cmd" == REDTRAIL_SKIP=1\ * || "$__rt_cmd" == *\ REDTRAIL_SKIP=1\ * ]]; then
        return $saved_exit
    fi

    # Blacklist check
    local cmd_str="$__rt_cmd"
    while :; do
        local word="${cmd_str%% *}"
        [[ "$word" == *=* ]] || break
        cmd_str="${cmd_str#"$word" }"
    done
    local binary="${cmd_str%% *}"
    binary="${binary##*/}"

    if [[ ":$__RT_BLACKLIST:" == *":$binary:"* ]]; then
        return $saved_exit
    fi

    # Resolve aliases so e.g. alias v=nvim is caught by the blacklist
    local alias_val
    if alias_val="$(alias "$binary" 2>/dev/null)"; then
        alias_val="${alias_val#*=}"
        alias_val="${alias_val#[\'\"]}"
        alias_val="${alias_val%[\'\"]}"
        local resolved="${alias_val%% *}"
        resolved="${resolved##*/}"
        if [[ ":$__RT_BLACKLIST:" == *":$resolved:"* ]]; then
            return $saved_exit
        fi
    fi

    # Register command with DB — get back a command ID for tee and finish
    __REDTRAIL_CMD_ID=$(command redtrail capture start \
        --session-id "$REDTRAIL_SESSION_ID" \
        --command "$__rt_cmd" \
        --cwd "$PWD" \
        --shell bash \
        --hostname "${HOSTNAME:-$(hostname)}" \
        2>/dev/null)
    [[ -z "$__REDTRAIL_CMD_ID" ]] && return $saved_exit

    # Set up capture
    local ctl_fifo="/tmp/rt-ctl-$$"
    mkfifo "$ctl_fifo" 2>/dev/null || return $saved_exit

    command redtrail tee \
        --command-id "$__REDTRAIL_CMD_ID" \
        --shell-pid "$$" \
        --ctl-fifo "$ctl_fifo" \
        2>/dev/null &
    __RT_TEE_PID=$!
    disown

    local pty_out pty_err
    if ! read -t 1 pty_out pty_err < "$ctl_fifo"; then
        rm -f "$ctl_fifo"
        kill "$__RT_TEE_PID" 2>/dev/null
        return $saved_exit
    fi
    rm -f "$ctl_fifo"

    exec {__RT_SAVE_OUT}>&1 {__RT_SAVE_ERR}>&2
    exec 1>"$pty_out" 2>"$pty_err"
    __RT_CAPTURE_ACTIVE=1
    return $saved_exit
}

__redtrail_precmd() {
    local exit_code=$?
    __RT_INSIDE_PRECMD=1

    if [[ -n "$__RT_CAPTURE_ACTIVE" ]]; then
        exec 1>&${__RT_SAVE_OUT} 2>&${__RT_SAVE_ERR}
        exec {__RT_SAVE_OUT}>&- {__RT_SAVE_ERR}>&-

        # Signal tee to flush and exit, then wait with timeout (poll — disowned jobs can't use wait)
        kill -USR1 "$__RT_TEE_PID" 2>/dev/null
        local __rt_w=0
        while kill -0 "$__RT_TEE_PID" 2>/dev/null; do
            sleep 0.01
            (( __rt_w++ )) || true
            (( __rt_w >= 200 )) && { kill -9 "$__RT_TEE_PID" 2>/dev/null; break; }
        done
    fi

    if [ -z "$__REDTRAIL_CMD_ID" ]; then
        unset __RT_INSIDE_PRECMD
        return
    fi

    # Finalize the command record — backgrounded to not block the prompt
    command redtrail capture finish \
        --command-id "$__REDTRAIL_CMD_ID" \
        --exit-code "$exit_code" \
        --cwd "$__REDTRAIL_CWD" \
        2>/dev/null & disown

    unset __REDTRAIL_CMD_ID __REDTRAIL_CWD
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
