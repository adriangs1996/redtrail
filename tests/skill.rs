use std::fs;
use std::process::Command;

#[test]
fn test_skill_init() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["skill", "init", "my-skill"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(tmp.path().join("my-skill/skill.toml").exists());
    assert!(tmp.path().join("my-skill/prompt.md").exists());
}

#[test]
fn test_skill_test_valid() {
    let tmp = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["skill", "init", "test-skill"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["skill", "test", "test-skill"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
}

#[test]
fn test_skill_test_invalid() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("bad-skill")).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["skill", "test", "bad-skill"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn test_skill_list_empty() {
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["skill", "list"])
        .output()
        .unwrap();
    assert!(out.status.success());
}
