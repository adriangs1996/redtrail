use crate::error::Error;
use std::path::Path;

/// Header metadata written to stdout/stderr capture temp files.
pub struct TempFileHeader {
    pub ts_start: i64,
    pub ts_end: i64,
    pub truncated: bool,
}

/// Write a capture temp file with header and content. File is created with mode 0600.
pub fn write_capture_file(
    path: &Path,
    header: &TempFileHeader,
    content: &str,
) -> Result<(), Error> {
    use std::io::Write;

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        write!(
            f,
            "ts_start:{}\nts_end:{}\ntruncated:{}\n\n{}",
            header.ts_start, header.ts_end, header.truncated, content
        )?;
    }

    #[cfg(not(unix))]
    {
        let mut f = std::fs::File::create(path)?;
        write!(
            f,
            "ts_start:{}\nts_end:{}\ntruncated:{}\n\n{}",
            header.ts_start, header.ts_end, header.truncated, content
        )?;
    }

    Ok(())
}

/// Read a capture temp file. Returns None if the file doesn't exist.
/// Returns (header, content) on success.
pub fn read_capture_file(path: &Path) -> Option<(TempFileHeader, String)> {
    let data = std::fs::read_to_string(path).ok()?;

    let mut ts_start: i64 = 0;
    let mut ts_end: i64 = 0;
    let mut truncated = false;

    let content_start = data.find("\n\n").map(|i| i + 2).unwrap_or(data.len());
    let header_section = &data[..content_start.saturating_sub(2).min(data.len())];

    for line in header_section.lines() {
        if let Some(val) = line.strip_prefix("ts_start:") {
            ts_start = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("ts_end:") {
            ts_end = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("truncated:") {
            truncated = val == "true";
        }
    }

    let content = if content_start < data.len() {
        data[content_start..].to_string()
    } else {
        String::new()
    };

    Some((
        TempFileHeader {
            ts_start,
            ts_end,
            truncated,
        },
        content,
    ))
}

/// Strip ANSI escape sequences from terminal output.
pub fn strip_ansi(input: &[u8]) -> String {
    let stripped = strip_ansi_escapes::strip(input);
    String::from_utf8_lossy(&stripped).to_string()
}
