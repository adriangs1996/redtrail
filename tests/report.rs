use std::process::Command;

fn setup_workspace_with_data() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let rt = env!("CARGO_BIN_EXE_rt");
    Command::new(rt)
        .args(["init", "--target", "10.10.10.1"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new(rt)
        .args([
            "sql",
            "INSERT OR IGNORE INTO hosts (session_id, ip, os) \
             SELECT id, '10.10.10.1', 'Linux' FROM sessions LIMIT 1",
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new(rt)
        .args([
            "sql",
            "INSERT OR IGNORE INTO ports (session_id, host_id, port, protocol, service) \
             SELECT s.id, h.id, 22, 'tcp', 'ssh' \
             FROM sessions s JOIN hosts h ON h.session_id = s.id LIMIT 1",
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new(rt)
        .args([
            "sql",
            "INSERT INTO flags (session_id, value, source) \
             SELECT id, 'HTB{test}', 'user.txt' FROM sessions LIMIT 1",
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new(rt)
        .args([
            "sql",
            "INSERT INTO hypotheses (session_id, statement, category, priority, confidence) \
             SELECT id, 'SQLi in login', 'I', 'high', 0.5 FROM sessions LIMIT 1",
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    tmp
}

#[test]
fn test_report_generate_stdout() {
    let tmp = setup_workspace_with_data();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["report", "generate"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
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
        .args([
            "report",
            "generate",
            "--output",
            report_path.to_str().unwrap(),
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(report_path.exists());
    let content = std::fs::read_to_string(&report_path).unwrap();
    assert!(content.contains("Penetration Test Report"));
}
