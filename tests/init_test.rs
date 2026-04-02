use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

#[test]
fn init_zsh_outputs_script_with_preexec_and_precmd() {
    let output = redtrail_bin()
        .args(["init", "zsh"])
        .output()
        .expect("failed to run redtrail");

    assert!(output.status.success(), "exit code should be 0");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("preexec"),
        "zsh script must define a preexec hook.\nGot:\n{stdout}"
    );
    assert!(
        stdout.contains("precmd"),
        "zsh script must define a precmd hook.\nGot:\n{stdout}"
    );
}

#[test]
fn init_zsh_sets_session_id() {
    let output = redtrail_bin()
        .args(["init", "zsh"])
        .output()
        .expect("failed to run redtrail");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("REDTRAIL_SESSION_ID"),
        "script must set REDTRAIL_SESSION_ID.\nGot:\n{stdout}"
    );
}

#[test]
fn init_bash_outputs_script_with_debug_trap_and_prompt_command() {
    let output = redtrail_bin()
        .args(["init", "bash"])
        .output()
        .expect("failed to run redtrail");

    assert!(output.status.success(), "exit code should be 0");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("DEBUG"),
        "bash script must use DEBUG trap.\nGot:\n{stdout}"
    );
    assert!(
        stdout.contains("PROMPT_COMMAND"),
        "bash script must set PROMPT_COMMAND.\nGot:\n{stdout}"
    );
}

#[test]
fn init_unknown_shell_fails() {
    let output = redtrail_bin()
        .args(["init", "powershell"])
        .output()
        .expect("failed to run redtrail");

    assert!(
        !output.status.success(),
        "unknown shell should fail with non-zero exit"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("zsh") && stderr.contains("bash"),
        "error should list supported shells.\nGot:\n{stderr}"
    );
}

#[test]
fn zsh_hook_contains_capture_start_and_finish() {
    let output = redtrail_bin()
        .args(["init", "zsh"])
        .output()
        .expect("failed to run");

    let hook = String::from_utf8_lossy(&output.stdout);
    assert!(
        hook.contains("capture start"),
        "zsh hook should call capture start"
    );
    assert!(
        hook.contains("capture finish"),
        "zsh hook should call capture finish"
    );
    assert!(
        hook.contains("--command-id"),
        "zsh hook should pass --command-id to tee and finish"
    );
    assert!(
        hook.contains("&!"),
        "zsh hook should background capture finish with &!"
    );
    assert!(
        hook.contains("__REDTRAIL_CMD_ID"),
        "zsh hook should use __REDTRAIL_CMD_ID variable"
    );
}

#[test]
fn zsh_hook_contains_fifo_setup() {
    let output = redtrail_bin()
        .args(["init", "zsh"])
        .output()
        .expect("failed to run");

    let hook = String::from_utf8_lossy(&output.stdout);
    assert!(hook.contains("mkfifo"), "zsh hook should create FIFO");
    assert!(
        hook.contains("redtrail tee"),
        "zsh hook should launch redtrail tee"
    );
    assert!(
        hook.contains("read -t 1"),
        "zsh hook should have FIFO read timeout"
    );
    assert!(
        hook.contains("__RT_BLACKLIST"),
        "zsh hook should have inline blacklist"
    );
}

#[test]
fn bash_hook_contains_capture_start_and_finish() {
    let output = redtrail_bin()
        .args(["init", "bash"])
        .output()
        .expect("failed to run");

    let hook = String::from_utf8_lossy(&output.stdout);
    assert!(
        hook.contains("capture start"),
        "bash hook should call capture start"
    );
    assert!(
        hook.contains("capture finish"),
        "bash hook should call capture finish"
    );
    assert!(
        hook.contains("--command-id"),
        "bash hook should pass --command-id to tee and finish"
    );
    assert!(
        hook.contains("disown"),
        "bash hook should use disown after capture finish"
    );
    assert!(
        hook.contains("__REDTRAIL_CMD_ID"),
        "bash hook should use __REDTRAIL_CMD_ID variable"
    );
}

#[test]
fn bash_hook_contains_fifo_setup() {
    let output = redtrail_bin()
        .args(["init", "bash"])
        .output()
        .expect("failed to run");

    let hook = String::from_utf8_lossy(&output.stdout);
    assert!(hook.contains("mkfifo"), "bash hook should create FIFO");
    assert!(
        hook.contains("redtrail tee"),
        "bash hook should launch redtrail tee"
    );
    assert!(
        hook.contains("read -t 1"),
        "bash hook should have FIFO read timeout"
    );
    assert!(
        hook.contains("history 1"),
        "bash hook should use history 1 for full command"
    );
    assert!(
        hook.contains("__RT_CAPTURE_ACTIVE"),
        "bash hook should have compound command guard"
    );
}

#[test]
fn hooks_do_not_use_date_nanoseconds() {
    for shell in &["zsh", "bash"] {
        let output = redtrail_bin()
            .args(["init", shell])
            .output()
            .expect("failed to run");

        let hook = String::from_utf8_lossy(&output.stdout);
        assert!(
            !hook.contains("date +%s%N"),
            "{shell} hook should NOT use date +%s%N (broken on macOS)"
        );
    }
}

#[test]
fn hooks_do_not_manage_timestamps() {
    for shell in &["zsh", "bash"] {
        let output = redtrail_bin()
            .args(["init", shell])
            .output()
            .expect("failed to run");

        let hook = String::from_utf8_lossy(&output.stdout);
        assert!(
            !hook.contains("--ts-start"),
            "{shell} hook should NOT pass --ts-start (DB handles timestamps)"
        );
        assert!(
            !hook.contains("--ts-end"),
            "{shell} hook should NOT pass --ts-end (DB handles timestamps)"
        );
        assert!(
            !hook.contains("__REDTRAIL_TS_START"),
            "{shell} hook should NOT use __REDTRAIL_TS_START (DB handles timestamps)"
        );
        assert!(
            !hook.contains("--stdout-file"),
            "{shell} hook should NOT use --stdout-file (tee streams to DB)"
        );
        assert!(
            !hook.contains("--stderr-file"),
            "{shell} hook should NOT use --stderr-file (tee streams to DB)"
        );
    }
}

#[test]
fn bash_hook_preserves_exit_code_in_debug_trap() {
    let output = redtrail_bin()
        .args(["init", "bash"])
        .output()
        .expect("failed to run");

    let hook = String::from_utf8_lossy(&output.stdout);
    assert!(
        hook.contains("saved_exit"),
        "bash DEBUG trap must save and restore $? to avoid clobbering exit codes"
    );
}

#[test]
fn zsh_hook_busy_waits_for_tee() {
    let output = redtrail_bin()
        .args(["init", "zsh"])
        .output()
        .expect("failed to run");

    let hook = String::from_utf8_lossy(&output.stdout);
    assert!(
        hook.contains("while kill -0"),
        "zsh hook must busy-wait for tee to exit before capture finish"
    );
    assert!(
        hook.contains("kill -USR1"),
        "zsh hook must signal tee with SIGUSR1 to flush"
    );
}

#[test]
fn bash_hook_busy_waits_for_tee() {
    let output = redtrail_bin()
        .args(["init", "bash"])
        .output()
        .expect("failed to run");

    let hook = String::from_utf8_lossy(&output.stdout);
    assert!(
        hook.contains("while kill -0"),
        "bash hook must busy-wait for tee to exit before capture finish"
    );
    assert!(
        hook.contains("kill -USR1"),
        "bash hook must signal tee with SIGUSR1 to flush"
    );
}
