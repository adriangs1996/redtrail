use std::process::Command;

fn setup_workspace() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["init", "--target", "10.10.10.1"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    tmp
}

#[test]
fn test_env_outputs_aliases() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["env"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("alias nmap='rt nmap'"),
        "should contain nmap alias"
    );
}

#[test]
fn test_env_exports_vars() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["env"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("RT_WORKSPACE="),
        "should export RT_WORKSPACE"
    );
    assert!(stdout.contains("RT_SESSION="), "should export RT_SESSION");
    assert!(
        stdout.contains("RT_TARGET='10.10.10.1'"),
        "should export RT_TARGET"
    );
}

#[test]
fn test_env_modifies_prompt() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["env"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("[rt:"),
        "should modify PS1 with session name"
    );
    assert!(stdout.contains("RT_OLD_PS1"), "should save old PS1");
}

#[test]
fn test_env_includes_deactivate_function() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["env"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("rt_deactivate()"),
        "should define deactivate function"
    );
    assert!(stdout.contains("unalias"), "deactivate should unalias");
    assert!(
        stdout.contains("unset RT_WORKSPACE"),
        "deactivate should unset vars"
    );
}

#[test]
fn test_deactivate_outputs_cleanup() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["deactivate"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("unalias"), "should unalias tools");
    assert!(
        stdout.contains("unset RT_WORKSPACE"),
        "should unset env vars"
    );
}
