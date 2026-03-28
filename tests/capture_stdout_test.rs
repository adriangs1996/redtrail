use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

fn now_ts() -> String {
    now_secs().to_string()
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn setup_db() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let _conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    dir
}

#[test]
fn capture_reads_stdout_from_file() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let now = now_secs();

    let stdout_file = dir.path().join("rt-out-test");
    redtrail::core::tee::write_capture_file(
        &stdout_file,
        &redtrail::core::tee::TempFileHeader {
            ts_start: now,
            ts_end: now + 1,
            truncated: false,
        },
        "hello from stdout\n",
    )
    .unwrap();

    let output = redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "echo hello",
            "--exit-code", "0",
            "--shell", "zsh",
            "--hostname", "devbox",
            "--stdout-file",
            stdout_file.to_str().unwrap(),
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(
        &conn,
        &redtrail::core::db::CommandFilter::default(),
    )
    .unwrap();

    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].stdout.as_deref(), Some("hello from stdout\n"));
    assert!(!cmds[0].stdout_truncated);

    // Temp file should be deleted by capture
    assert!(!stdout_file.exists(), "capture should delete the temp file");
}

#[test]
fn capture_reads_stderr_from_file() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let now = now_secs();

    let stderr_file = dir.path().join("rt-err-test");
    redtrail::core::tee::write_capture_file(
        &stderr_file,
        &redtrail::core::tee::TempFileHeader {
            ts_start: now,
            ts_end: now + 1,
            truncated: true,
        },
        "error output\n",
    )
    .unwrap();

    let output = redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "make build",
            "--exit-code", "1",
            "--shell", "zsh",
            "--hostname", "devbox",
            "--stderr-file",
            stderr_file.to_str().unwrap(),
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(
        &conn,
        &redtrail::core::db::CommandFilter::default(),
    )
    .unwrap();

    assert_eq!(cmds[0].stderr.as_deref(), Some("error output\n"));
    assert!(cmds[0].stderr_truncated);
    assert!(!stderr_file.exists());
}

#[test]
fn capture_uses_timestamps_from_temp_file_header() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let now = now_secs();

    let stdout_file = dir.path().join("rt-out-ts");
    redtrail::core::tee::write_capture_file(
        &stdout_file,
        &redtrail::core::tee::TempFileHeader {
            ts_start: now,
            ts_end: now + 2000,
            truncated: false,
        },
        "output",
    )
    .unwrap();

    redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "echo test",
            "--exit-code", "0",
            "--shell", "zsh",
            "--hostname", "devbox",
            "--stdout-file",
            stdout_file.to_str().unwrap(),
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(
        &conn,
        &redtrail::core::db::CommandFilter::default(),
    )
    .unwrap();

    assert_eq!(cmds[0].timestamp_start, now);
    assert_eq!(cmds[0].timestamp_end, Some(now + 2000));
}

#[test]
fn capture_without_files_still_works() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let ts = now_ts();

    let output = redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "ls",
            "--exit-code", "0",
            "--ts-start", &ts,
            "--shell", "zsh",
            "--hostname", "devbox",
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    assert!(output.status.success());

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(
        &conn,
        &redtrail::core::db::CommandFilter::default(),
    )
    .unwrap();

    assert_eq!(cmds.len(), 1);
    assert!(cmds[0].stdout.is_none());
    assert!(cmds[0].stderr.is_none());
    let expected_ts: i64 = ts.parse().unwrap();
    assert_eq!(cmds[0].timestamp_start, expected_ts);
}

#[test]
fn capture_redacts_secrets_in_stdout_file() {
    let dir = setup_db();
    let db_path = dir.path().join("test.db");
    let now = now_secs();

    let stdout_file = dir.path().join("rt-out-secret");
    redtrail::core::tee::write_capture_file(
        &stdout_file,
        &redtrail::core::tee::TempFileHeader {
            ts_start: now,
            ts_end: now + 1,
            truncated: false,
        },
        "aws_access_key_id=AKIAIOSFODNN7EXAMPLE\n",
    )
    .unwrap();

    redtrail_bin()
        .args([
            "capture",
            "--session-id", "s1",
            "--command", "cat credentials",
            "--exit-code", "0",
            "--shell", "zsh",
            "--hostname", "devbox",
            "--stdout-file",
            stdout_file.to_str().unwrap(),
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("failed to run");

    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(
        &conn,
        &redtrail::core::db::CommandFilter::default(),
    )
    .unwrap();

    let stdout = cmds[0].stdout.as_ref().unwrap();
    assert!(!stdout.contains("AKIAIOSFODNN7EXAMPLE"), "secret should be redacted in stdout");
    assert!(cmds[0].redacted);
}
