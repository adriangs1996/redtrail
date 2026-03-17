use std::process::Command;

fn setup_workspace_with_data() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let rt = env!("CARGO_BIN_EXE_rt");
    Command::new(rt).args(["init", "--target", "10.10.10.1"]).current_dir(tmp.path()).output().unwrap();
    Command::new(rt).args(["kb", "add-host", "10.10.10.1", "--os", "Linux"]).current_dir(tmp.path()).output().unwrap();
    Command::new(rt).args(["kb", "add-port", "10.10.10.1", "22", "--service", "ssh"]).current_dir(tmp.path()).output().unwrap();
    Command::new(rt).args(["kb", "add-flag", "HTB{test}", "--source", "user.txt"]).current_dir(tmp.path()).output().unwrap();
    Command::new(rt).args(["hypothesis", "create", "SQLi in login", "--category", "I", "--priority", "high"]).current_dir(tmp.path()).output().unwrap();
    tmp
}

#[test]
fn test_report_generate_stdout() {
    let tmp = setup_workspace_with_data();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["report", "generate"])
        .current_dir(tmp.path())
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Penetration Test Report"));
    assert!(stdout.contains("10.10.10.1"));
    assert!(stdout.contains("HTB{test}"));
    assert!(stdout.contains("SQLi in login"));
}

#[test]
fn test_report_generate_to_file() {
    let tmp = setup_workspace_with_data();
    let report_path = tmp.path().join("report.md");
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["report", "generate", "--output", report_path.to_str().unwrap()])
        .current_dir(tmp.path())
        .output().unwrap();
    assert!(out.status.success());
    assert!(report_path.exists());
    let content = std::fs::read_to_string(&report_path).unwrap();
    assert!(content.contains("Penetration Test Report"));
}
