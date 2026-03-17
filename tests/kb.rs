use std::process::Command;

fn setup_workspace() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["init", "--target", "10.10.10.1"])
        .current_dir(tmp.path())
        .output().unwrap();
    tmp
}

#[test]
fn test_add_and_list_host() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "add-host", "10.10.10.1", "--os", "Linux"])
        .current_dir(tmp.path())
        .output().unwrap();
    assert!(out.status.success());

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "hosts", "--json"])
        .current_dir(tmp.path())
        .output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["ip"], "10.10.10.1");
    assert_eq!(json[0]["os"], "Linux");
}

#[test]
fn test_add_and_list_flag() {
    let tmp = setup_workspace();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "add-flag", "HTB{test123}", "--source", "user.txt"])
        .current_dir(tmp.path())
        .output().unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "flags", "--json"])
        .current_dir(tmp.path())
        .output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["value"], "HTB{test123}");
}

#[test]
fn test_status_json() {
    let tmp = setup_workspace();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "add-host", "10.10.10.1"])
        .current_dir(tmp.path())
        .output().unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["status", "--json"])
        .current_dir(tmp.path())
        .output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["hosts"], 1);
    assert_eq!(json["target"], "10.10.10.1");
}

#[test]
fn test_add_port_auto_creates_host() {
    let tmp = setup_workspace();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "add-port", "10.10.10.1", "22", "--service", "ssh"])
        .current_dir(tmp.path())
        .output().unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "hosts", "--json"])
        .current_dir(tmp.path())
        .output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["ip"], "10.10.10.1");

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "ports", "--json"])
        .current_dir(tmp.path())
        .output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["port"], 22);
    assert_eq!(json[0]["service"], "ssh");
}

#[test]
fn test_search() {
    let tmp = setup_workspace();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "add-host", "10.10.10.1", "--hostname", "target-box"])
        .current_dir(tmp.path())
        .output().unwrap();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "add-note", "found ssh on target-box"])
        .current_dir(tmp.path())
        .output().unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "search", "target", "--json"])
        .current_dir(tmp.path())
        .output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json.as_array().unwrap().len() >= 2);
}
