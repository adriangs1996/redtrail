use std::process::Command;
use std::fs;

fn setup_workspace() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["init", "--target", "10.10.10.1"])
        .current_dir(tmp.path())
        .output().unwrap();
    tmp
}

#[test]
fn test_ingest_file() {
    let tmp = setup_workspace();
    let scan_file = tmp.path().join("scan.txt");
    fs::write(&scan_file, "Nmap scan report for 10.10.10.1\nPORT   STATE SERVICE\n22/tcp open  ssh\n80/tcp open  http\n").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["ingest", scan_file.to_str().unwrap()])
        .current_dir(tmp.path())
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ingested"));
    assert!(stdout.contains("nmap"));

    let hist = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "history", "--json"])
        .current_dir(tmp.path())
        .output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&hist.stdout).unwrap();
    let arr = json.as_array().unwrap();
    assert!(!arr.is_empty());
    assert!(arr.iter().any(|e| e["tool"].as_str() == Some("nmap")));
}

#[test]
fn test_ingest_with_tool_override() {
    let tmp = setup_workspace();
    let file = tmp.path().join("output.txt");
    fs::write(&file, "some custom scanner output").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["ingest", file.to_str().unwrap(), "--tool", "my-scanner"])
        .current_dir(tmp.path())
        .output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("my-scanner"));
}
