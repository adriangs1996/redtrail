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
fn test_kb_extract_without_api_key() {
    let tmp = setup_workspace();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["echo", "PORT STATE SERVICE\n22/tcp open ssh"])
        .current_dir(tmp.path())
        .output().unwrap();

    let hist = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "history", "--json"])
        .current_dir(tmp.path())
        .output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&hist.stdout).unwrap();
    if json.as_array().unwrap().is_empty() { return; }
    let id = json[0]["id"].as_i64().unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "extract", &id.to_string()])
        .env_remove("ANTHROPIC_API_KEY")
        .current_dir(tmp.path())
        .output().unwrap();
    assert!(!out.status.success() || !String::from_utf8_lossy(&out.stderr).is_empty());
}
