use std::process::Command;

#[test]
fn test_setup_status_json() {
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["setup", "status", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json.get("installed").is_some());
    assert!(json.get("shell").is_some());
}

#[test]
fn test_setup_status_shows_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["init", "--target", "10.10.10.1"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["setup", "status", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json["active_workspace"].as_str().is_some());
}

#[test]
fn test_setup_aliases_list() {
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["setup", "aliases", "--list"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("nmap"));
}
