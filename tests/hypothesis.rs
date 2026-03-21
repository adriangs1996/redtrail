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
fn test_hypothesis_list_empty() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["hypothesis", "list", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json.as_array().unwrap().is_empty());
}

#[test]
fn test_evidence_list_empty() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["evidence", "list", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json.as_array().unwrap().is_empty());
}

#[test]
fn test_evidence_export_empty() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["evidence", "export", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
}
