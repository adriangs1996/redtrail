use redtrail::core::tee::{TempFileHeader, write_capture_file, read_capture_file, strip_ansi};

#[test]
fn write_and_read_capture_file_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rt-out-12345");

    let header = TempFileHeader {
        ts_start: 1711555200,
        ts_end: 1711555203,
        truncated: false,
    };
    write_capture_file(&path, &header, "hello world\n").unwrap();

    let (h, content) = read_capture_file(&path).unwrap();
    assert_eq!(h.ts_start, 1711555200i64);
    assert_eq!(h.ts_end, 1711555203i64);
    assert!(!h.truncated);
    assert_eq!(content, "hello world\n");
}

#[test]
fn read_capture_file_with_truncated_flag() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rt-out-12345");

    let header = TempFileHeader {
        ts_start: 1000,
        ts_end: 2000,
        truncated: true,
    };
    write_capture_file(&path, &header, "partial output").unwrap();

    let (h, content) = read_capture_file(&path).unwrap();
    assert!(h.truncated);
    assert_eq!(content, "partial output");
}

#[test]
fn read_capture_file_returns_none_for_missing_file() {
    let result = read_capture_file(std::path::Path::new("/tmp/nonexistent-rt-file"));
    assert!(result.is_none());
}

#[test]
fn write_capture_file_sets_permissions_to_600() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rt-out-perms");

    let header = TempFileHeader {
        ts_start: 1000,
        ts_end: 2000,
        truncated: false,
    };
    write_capture_file(&path, &header, "secret output").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }
}

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
fn read_capture_file_with_empty_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rt-out-empty");

    let header = TempFileHeader {
        ts_start: 1000,
        ts_end: 2000,
        truncated: false,
    };
    write_capture_file(&path, &header, "").unwrap();

    let (h, content) = read_capture_file(&path).unwrap();
    assert_eq!(h.ts_start, 1000);
    assert_eq!(content, "");
}
