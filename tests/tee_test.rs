use redtrail::core::tee::{allocate_pty_pair, strip_ansi};
use std::os::fd::AsRawFd;

#[test]
fn strip_ansi_removes_color_codes() {
    let colored = "\x1b[32mgreen text\x1b[0m and normal";
    let stripped = strip_ansi(colored.as_bytes());
    assert_eq!(stripped, "green text and normal");
}

#[test]
fn strip_ansi_handles_plain_text() {
    let plain = "no escape codes here";
    let stripped = strip_ansi(plain.as_bytes());
    assert_eq!(stripped, "no escape codes here");
}

#[test]
fn pty_allocation_creates_valid_pair() {
    let pty = allocate_pty_pair().expect("PTY allocation should succeed");

    assert!(
        std::path::Path::new(&pty.slave_path).exists(),
        "slave path should exist: {}",
        pty.slave_path
    );

    assert!(pty.master_fd.as_raw_fd() >= 0);
}

#[test]
fn pty_relay_captures_output() {
    use std::io::Write;

    let pty = allocate_pty_pair().expect("PTY allocation should succeed");

    let mut slave_file = unsafe {
        use std::os::fd::FromRawFd;
        let dup_fd = nix::unistd::dup(pty.slave_fd.as_raw_fd()).unwrap();
        std::fs::File::from_raw_fd(dup_fd)
    };
    slave_file.write_all(b"hello from pty\n").unwrap();
    slave_file.flush().unwrap();

    let mut buf = vec![0u8; 1024];
    let n = nix::unistd::read(pty.master_fd.as_raw_fd(), &mut buf).unwrap();

    assert!(n > 0, "expected data from PTY master, got 0 bytes");
    let output = String::from_utf8_lossy(&buf[..n]);
    assert!(output.contains("hello from pty"), "got: {output}");
}
