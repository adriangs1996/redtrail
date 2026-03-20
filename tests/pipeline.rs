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
fn test_flag_detection_in_proxy() {
    let tmp = setup_workspace();
    let output = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["echo", "found HTB{test_flag_123}"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(output.status.success());

    let flags_out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "flags", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&flags_out.stdout).unwrap();
    let arr = json.as_array().unwrap();
    assert!(!arr.is_empty(), "flag should have been auto-detected");
    assert!(arr[0]["value"].as_str().unwrap().contains("HTB{"));
}

#[test]
fn test_noise_budget_decrements() {
    let tmp = setup_workspace();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["echo", "test"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    let status_out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["status", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&status_out.stdout).unwrap();
    let budget: f64 = json["noise_budget"].as_f64().unwrap();
    assert!(
        budget < 1.0,
        "noise budget should have decreased from 1.0, got {budget}"
    );
}
