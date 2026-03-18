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
fn test_proxy_captures_output() {
    let tmp = setup_workspace();
    let output = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["echo", "hello world"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello"), "stdout should contain 'hello', got: {stdout}");

    let db_out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "history", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&db_out.stdout).unwrap();
    let arr = json.as_array().unwrap();
    assert!(!arr.is_empty(), "command_history should have entries");
    assert!(arr[0]["command"].as_str().unwrap().contains("echo"));
    assert_eq!(arr[0]["exit_code"], 0);
}

#[test]
fn test_proxy_captures_exit_code() {
    let tmp = setup_workspace();
    let output = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["sh", "-c", "exit 42"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(42));

    let db_out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "history", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&db_out.stdout).unwrap();
    let arr = json.as_array().unwrap();
    assert!(!arr.is_empty(), "command_history should have entries");
    assert_eq!(arr[0]["exit_code"], 42);
}

#[test]
fn test_proxy_double_dash() {
    let tmp = setup_workspace();
    let output = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["--", "echo", "via double dash"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("via double dash"));
}

#[test]
fn test_proxy_without_workspace_still_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["echo", "no workspace"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("no workspace"));
}
