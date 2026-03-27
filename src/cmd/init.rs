use crate::error::Error;

const ZSH_HOOK: &str = r#"
# RedTrail shell integration — zsh
# eval "$(redtrail init zsh)"

export REDTRAIL_SESSION_ID="$(command redtrail session-id 2>/dev/null || echo "rt-$$-$(date +%s)")"

__redtrail_preexec() {
    export __REDTRAIL_CMD="$1"
    export __REDTRAIL_CWD="$PWD"
    export __REDTRAIL_TS_START="$(date +%s)"
}

__redtrail_precmd() {
    local exit_code=$?
    [ -z "$__REDTRAIL_CMD" ] && return

    local ts_end
    ts_end="$(date +%s)"

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
}

autoload -Uz add-zsh-hook
add-zsh-hook preexec __redtrail_preexec
add-zsh-hook precmd __redtrail_precmd
"#;

const BASH_HOOK: &str = r#"
# RedTrail shell integration — bash
# eval "$(redtrail init bash)"

export REDTRAIL_SESSION_ID="$(command redtrail session-id 2>/dev/null || echo "rt-$$-$(date +%s)")"

__redtrail_preexec() {
    [ -n "$COMP_LINE" ] && return
    [ "$BASH_COMMAND" = "$PROMPT_COMMAND" ] && return

    export __REDTRAIL_CMD="$BASH_COMMAND"
    export __REDTRAIL_CWD="$PWD"
    export __REDTRAIL_TS_START="$(date +%s)"
}

__redtrail_precmd() {
    local exit_code=$?
    [ -z "$__REDTRAIL_CMD" ] && return

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
