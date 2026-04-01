use std::io::Write;
use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

/// Test that tee captures output and streams it to the DB.
///
/// 1. Creates a temp DB and a session + running command via `capture start`
/// 2. Creates a FIFO
/// 3. Starts `redtrail tee` with `--command-id`
/// 4. Reads PTY slave paths from the FIFO
/// 5. Writes to the stdout PTY slave
/// 6. Signals SIGUSR1 to flush
/// 7. Verifies the DB row has the captured output
///
/// Note: This test requires /dev/tty (a real terminal).
#[test]
fn tee_streams_output_to_db() {
    if std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/tty")
        .is_err()
    {
        eprintln!("skipping tee_streams_output_to_db: no /dev/tty");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let fifo_path = dir.path().join("ctl-fifo");
    let shell_pid = format!("tee-test-{}", std::process::id());

    // Create DB and a running command via capture start
    let start_output = redtrail_bin()
        .args([
            "capture", "start",
            "--session-id", "test-sess",
            "--command", "echo hello",
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("capture start");
    assert!(
        start_output.status.success(),
        "capture start failed: {}",
        String::from_utf8_lossy(&start_output.stderr)
    );
    let command_id = String::from_utf8_lossy(&start_output.stdout).trim().to_string();
    assert!(!command_id.is_empty(), "capture start should print command_id");

    nix::unistd::mkfifo(
        &fifo_path,
        nix::sys::stat::Mode::from_bits_truncate(0o600),
    )
    .expect("mkfifo");

    // Start tee in background
    let mut child = redtrail_bin()
        .args([
            "tee",
            "--command-id", &command_id,
            "--shell-pid", &shell_pid,
            "--ctl-fifo", fifo_path.to_str().unwrap(),
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .spawn()
        .expect("failed to start tee");

    // Read PTY paths from FIFO
    let fifo_content = std::fs::read_to_string(&fifo_path).unwrap();
    let paths: Vec<&str> = fifo_content.trim().split_whitespace().collect();
    assert_eq!(
        paths.len(),
        2,
        "should get two PTY slave paths, got: {fifo_content}"
    );

    // Write to the stdout PTY slave, then close it
    {
        let mut slave = std::fs::OpenOptions::new()
            .write(true)
            .open(paths[0])
            .unwrap();
        slave.write_all(b"captured output\n").unwrap();
    }

    // Open and close stderr slave to signal EOF on both
    {
        let _ = std::fs::OpenOptions::new().write(true).open(paths[1]);
    }

    // Signal tee to flush and exit
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGUSR1,
    )
    .ok();

    let status = child.wait().expect("wait failed");
    assert!(status.success(), "tee should exit cleanly");

    // Verify DB has the captured output
    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(
        &conn,
        &redtrail::core::db::CommandFilter::default(),
    )
    .unwrap();

    assert_eq!(cmds.len(), 1, "should have one command");
    let stdout = cmds[0].stdout.as_ref().expect("stdout should be captured");
    assert!(
        stdout.contains("captured output"),
        "stdout should contain our output: {stdout}"
    );
}

/// End-to-end: capture start -> tee -> capture finish -> verify DB
#[test]
fn end_to_end_tee_then_capture_finish() {
    if std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/tty")
        .is_err()
    {
        eprintln!("skipping end_to_end_tee_then_capture_finish: no /dev/tty");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let fifo_path = dir.path().join("ctl-fifo");
    let shell_pid = format!("e2e-{}", std::process::id());

    // Step 1: capture start
    let start_output = redtrail_bin()
        .args([
            "capture", "start",
            "--session-id", "e2e-sess",
            "--command", "make build",
            "--shell", "zsh",
            "--hostname", "devbox",
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("capture start");
    assert!(start_output.status.success(), "capture start failed: {}", String::from_utf8_lossy(&start_output.stderr));
    let command_id = String::from_utf8_lossy(&start_output.stdout).trim().to_string();

    nix::unistd::mkfifo(
        &fifo_path,
        nix::sys::stat::Mode::from_bits_truncate(0o600),
    )
    .expect("mkfifo");

    // Step 2: start tee
    let mut tee_child = redtrail_bin()
        .args([
            "tee",
            "--command-id", &command_id,
            "--shell-pid", &shell_pid,
            "--ctl-fifo", fifo_path.to_str().unwrap(),
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .spawn()
        .expect("start tee");

    // Read PTY paths
    let fifo_content = std::fs::read_to_string(&fifo_path).unwrap();
    let paths: Vec<&str> = fifo_content.trim().split_whitespace().collect();

    // Simulate command writing to stdout PTY
    {
        let mut slave = std::fs::OpenOptions::new()
            .write(true)
            .open(paths[0])
            .unwrap();
        slave.write_all(b"build output line 1\nbuild output line 2\n").unwrap();
    }
    // Close stderr slave
    {
        let _ = std::fs::OpenOptions::new().write(true).open(paths[1]);
    }

    // Signal tee to flush and exit
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(tee_child.id() as i32),
        nix::sys::signal::Signal::SIGUSR1,
    )
    .ok();

    tee_child.wait().expect("tee wait");

    // Step 3: capture finish
    let finish_output = redtrail_bin()
        .args([
            "capture", "finish",
            "--command-id", &command_id,
            "--exit-code", "0",
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("capture finish");
    assert!(finish_output.status.success(), "capture finish failed: {}", String::from_utf8_lossy(&finish_output.stderr));

    // Verify DB
    let conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();
    let cmds = redtrail::core::db::get_commands(
        &conn,
        &redtrail::core::db::CommandFilter::default(),
    )
    .unwrap();

    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].command_raw, "make build");
    let stdout = cmds[0].stdout.as_ref().expect("stdout should be captured");
    assert!(stdout.contains("build output line 1"), "stdout: {stdout}");
    assert!(stdout.contains("build output line 2"), "stdout: {stdout}");
    assert_eq!(cmds[0].exit_code, Some(0));
    assert!(cmds[0].timestamp_start > 0);
    assert!(cmds[0].timestamp_end.is_some(), "ts_end should be set by finish");
}
