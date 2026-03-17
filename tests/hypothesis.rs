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
fn test_create_and_list_hypothesis() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["hypothesis", "create", "SQLi in login form", "--category", "I", "--priority", "high", "--confidence", "0.7"])
        .current_dir(tmp.path())
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["hypothesis", "list", "--json"])
        .current_dir(tmp.path())
        .output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["statement"], "SQLi in login form");
    assert_eq!(json[0]["category"], "I");
    assert_eq!(json[0]["status"], "pending");
}

#[test]
fn test_update_hypothesis() {
    let tmp = setup_workspace();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["hypothesis", "create", "test hyp", "--category", "B"])
        .current_dir(tmp.path())
        .output().unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["hypothesis", "list", "--json"])
        .current_dir(tmp.path())
        .output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = json[0]["id"].as_i64().unwrap();

    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["hypothesis", "update", &id.to_string(), "--status", "confirmed"])
        .current_dir(tmp.path())
        .output().unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["hypothesis", "list", "--status", "confirmed", "--json"])
        .current_dir(tmp.path())
        .output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["status"], "confirmed");
}

#[test]
fn test_add_and_list_evidence() {
    let tmp = setup_workspace();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["hypothesis", "create", "SSTI in template", "--category", "I"])
        .current_dir(tmp.path())
        .output().unwrap();

    let hyp_out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["hypothesis", "list", "--json"])
        .current_dir(tmp.path())
        .output().unwrap();
    let hyp_json: serde_json::Value = serde_json::from_slice(&hyp_out.stdout).unwrap();
    let hyp_id = hyp_json[0]["id"].as_i64().unwrap();

    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["evidence", "add", "--finding", "SSTI confirmed with {{7*7}}=49",
               "--hypothesis", &hyp_id.to_string(),
               "--severity", "critical",
               "--poc", "curl http://target/page?name={{7*7}}"])
        .current_dir(tmp.path())
        .output().unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["evidence", "list", "--json"])
        .current_dir(tmp.path())
        .output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["severity"], "critical");
    assert!(json[0]["finding"].as_str().unwrap().contains("SSTI"));
}

#[test]
fn test_evidence_export() {
    let tmp = setup_workspace();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["evidence", "add", "--finding", "open port 22", "--severity", "info"])
        .current_dir(tmp.path())
        .output().unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["evidence", "export", "--json"])
        .current_dir(tmp.path())
        .output().unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!json.as_array().unwrap().is_empty());
}
