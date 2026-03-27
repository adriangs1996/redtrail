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
