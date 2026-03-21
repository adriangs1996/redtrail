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
fn test_hosts_empty() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "hosts", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json.as_array().unwrap().is_empty());
}

#[test]
fn test_status_json() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["status", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["target"], "10.10.10.1");
}

#[test]
fn test_paths_cli_empty() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "paths", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[test]
fn test_vulns_cli_empty() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "vulns", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[test]
fn test_search_empty() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "search", "nothing", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json.as_array().unwrap().is_empty());
}
