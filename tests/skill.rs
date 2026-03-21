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

#[test]
fn test_skill_test_valid_tools_field() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("tool-skill");
    fs::create_dir(&dir).unwrap();
    fs::write(
        dir.join("skill.toml"),
        "name = \"tool-skill\"\ntools = [\"query_table\", \"suggest\"]\n",
    )
    .unwrap();
    fs::write(dir.join("prompt.md"), "# Tool skill\nContent here.\n").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["skill", "test", "tool-skill"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn test_skill_test_invalid_tools_field() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("bad-tool-skill");
    fs::create_dir(&dir).unwrap();
    fs::write(
        dir.join("skill.toml"),
        "name = \"bad-tool-skill\"\ntools = [\"nonexistent_tool\"]\n",
    )
    .unwrap();
    fs::write(dir.join("prompt.md"), "# Bad tool skill\nContent.\n").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["skill", "test", "bad-tool-skill"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown tool"));
}

#[test]
fn test_skill_test_no_tools_field_still_valid() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("no-tools");
    fs::create_dir(&dir).unwrap();
    fs::write(
        dir.join("skill.toml"),
        "name = \"no-tools\"\n",
    )
    .unwrap();
    fs::write(dir.join("prompt.md"), "# No tools\nContent.\n").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["skill", "test", "no-tools"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
