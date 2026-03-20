use std::process::Command;

fn setup_workspace() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["init", "--target", "10.10.10.1", "--scope", "10.10.10.0/24"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    tmp
}

#[test]
fn test_session_active() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["session", "active", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["target"], "10.10.10.1");
}

#[test]
fn test_session_list() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["session", "list", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json.as_array().unwrap().len() == 1);
}

#[test]
fn test_session_export() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["session", "export"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json["session"]["target"] == "10.10.10.1");
}

#[test]
fn test_scope_check_in_scope() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["scope", "check", "10.10.10.5"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("in-scope"));
}

#[test]
fn test_scope_check_out_of_scope() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["scope", "check", "192.168.1.1"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("out-of-scope"));
}

#[test]
fn test_config_list() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["config", "list"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("autonomy"));
}

#[test]
fn test_config_get() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["config", "get", "general.autonomy"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("balanced"));
}

#[test]
fn test_config_set() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["config", "set", "general.autonomy", "passive"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out2 = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["config", "get", "general.autonomy"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out2.status.success());
    let stdout = String::from_utf8_lossy(&out2.stdout);
    assert!(stdout.contains("passive"));
}

#[test]
fn test_pipeline_stub() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["pipeline"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("v2"));
}
