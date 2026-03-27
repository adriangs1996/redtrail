use std::io::Write;
use std::process::Command;

fn redtrail_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redtrail"))
}

/// This test exercises the `redtrail tee` subcommand:
/// 1. Creates a FIFO
/// 2. Starts `redtrail tee` in background
/// 3. Reads PTY slave paths from the FIFO
/// 4. Writes to the stdout PTY slave
/// 5. Closes both slaves (triggers EOF)
/// 6. Verifies temp files were created with correct content
///
/// Note: This test requires /dev/tty (a real terminal). It will fail in
/// environments without a controlling terminal (e.g., some CI setups).
#[test]
fn tee_creates_pty_and_captures_output() {
    // Skip if no controlling terminal
    if std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/tty")
        .is_err()
    {
        eprintln!("skipping tee_creates_pty_and_captures_output: no /dev/tty");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let fifo_path = dir.path().join("ctl-fifo");
    let shell_pid = format!("tee-test-{}", std::process::id());

    nix::unistd::mkfifo(
        &fifo_path,
        nix::sys::stat::Mode::from_bits_truncate(0o600),
    )
    .expect("mkfifo");

    // Start tee in background
    let mut child = redtrail_bin()
        .args([
            "tee",
            "--session",
            "test-sess",
            "--shell-pid",
            &shell_pid,
            "--ctl-fifo",
            fifo_path.to_str().unwrap(),
        ])
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

    // Wait for tee to exit
    let status = child.wait().expect("wait failed");
    assert!(status.success(), "tee should exit cleanly");

    // Check temp files
    let out_file = format!("/tmp/rt-out-{shell_pid}");
    assert!(
        std::path::Path::new(&out_file).exists(),
        "stdout temp file should exist at {out_file}"
    );

    let (header, content) =
        redtrail::core::tee::read_capture_file(std::path::Path::new(&out_file)).unwrap();
    assert!(
        content.contains("captured output"),
        "content should contain our output: {content}"
    );
    assert!(!header.truncated);
    assert!(header.ts_start > 0);
    assert!(header.ts_end >= header.ts_start);

    // Cleanup
    let _ = std::fs::remove_file(&out_file);
    let _ = std::fs::remove_file(format!("/tmp/rt-err-{shell_pid}"));
}

#[test]
fn end_to_end_tee_then_capture() {
    // Skip if no controlling terminal
    if std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/tty")
        .is_err()
    {
        eprintln!("skipping end_to_end_tee_then_capture: no /dev/tty");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let _conn = redtrail::core::db::open(db_path.to_str().unwrap()).unwrap();

    let fifo_path = dir.path().join("ctl-fifo");
    let shell_pid = format!("e2e-{}", std::process::id());

    nix::unistd::mkfifo(
        &fifo_path,
        nix::sys::stat::Mode::from_bits_truncate(0o600),
    )
    .expect("mkfifo");

    // Start tee
    let mut tee_child = redtrail_bin()
        .args([
            "tee",
            "--session", "e2e-sess",
            "--shell-pid", &shell_pid,
            "--ctl-fifo", fifo_path.to_str().unwrap(),
        ])
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

    // Wait for tee to finish
    tee_child.wait().expect("tee wait");

    // Now run capture with the temp files
    let out_file = format!("/tmp/rt-out-{}", shell_pid);
    let output = redtrail_bin()
        .args([
            "capture",
            "--session-id", "e2e-sess",
            "--command", "make build",
            "--exit-code", "0",
            "--shell", "zsh",
            "--hostname", "devbox",
            "--stdout-file", &out_file,
        ])
        .env("REDTRAIL_DB", db_path.to_str().unwrap())
        .output()
        .expect("capture");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

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
    assert!(cmds[0].timestamp_start > 0, "timestamp should come from tee");
    assert!(cmds[0].timestamp_end.is_some(), "ts_end should come from tee");

    // Temp files should be cleaned up by capture
    assert!(!std::path::Path::new(&out_file).exists());

    // Clean up any remaining
    let _ = std::fs::remove_file(format!("/tmp/rt-err-{}", shell_pid));
}
