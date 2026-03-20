use std::process::Command;

#[test]
fn test_rt_init_creates_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["init", "--target", "10.10.10.1", "--goal", "capture-flags"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(tmp.path().join(".redtrail").is_dir());
    assert!(tmp.path().join(".redtrail/redtrail.db").exists());
    assert!(tmp.path().join(".redtrail/config.toml").exists());
    assert!(tmp.path().join(".redtrail/aliases.sh").exists());
}

#[test]
fn test_rt_init_aliases_contain_nmap() {
    let tmp = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["init"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let aliases = std::fs::read_to_string(tmp.path().join(".redtrail/aliases.sh")).unwrap();
    assert!(aliases.contains("alias nmap='rt nmap'"));
}

#[test]
fn test_rt_init_twice_warns() {
    let tmp = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["init"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["init"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already exists"));
}
