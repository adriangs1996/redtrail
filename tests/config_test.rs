use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

#[test]
fn config_view_shows_yaml() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yaml");

    let output = redtrail_bin()
        .args(["config"])
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should show default config in YAML-like format
    assert!(stdout.contains("capture") || stdout.contains("secrets") || stdout.contains("redact"),
        "should show config keys, got:\n{stdout}");
}

#[test]
fn config_set_updates_value() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yaml");

    // Set a value
    let output = redtrail_bin()
        .args(["config", "set", "capture.max_stdout_bytes", "102400"])
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    // Verify it persisted
    let output2 = redtrail_bin()
        .args(["config"])
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output2.stdout);
    assert!(stdout.contains("102400"), "should show updated value, got:\n{stdout}");
}

#[test]
fn config_set_secrets_on_detect() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yaml");

    // Set on_detect to warn
    let output = redtrail_bin()
        .args(["config", "set", "secrets.on_detect", "warn"])
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    // Verify it persisted
    let output2 = redtrail_bin()
        .args(["config"])
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output2.stdout);
    assert!(stdout.contains("warn"), "should show on_detect=warn, got:\n{stdout}");
}

#[test]
fn config_set_secrets_on_detect_rejects_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yaml");

    let output = redtrail_bin()
        .args(["config", "set", "secrets.on_detect", "delete_everything"])
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");
    assert!(!output.status.success(), "should reject invalid on_detect value");
}

#[test]
fn config_set_secrets_patterns_file() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yaml");

    let output = redtrail_bin()
        .args(["config", "set", "secrets.patterns_file", "/home/user/.redtrail/patterns.yaml"])
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let output2 = redtrail_bin()
        .args(["config"])
        .env("REDTRAIL_CONFIG", config_path.to_str().unwrap())
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output2.stdout);
    assert!(stdout.contains("patterns.yaml"), "should show patterns_file path, got:\n{stdout}");
}
