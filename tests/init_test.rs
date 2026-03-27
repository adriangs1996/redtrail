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
