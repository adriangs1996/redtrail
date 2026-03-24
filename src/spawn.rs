use crate::error::Error;
use std::process::{Command, Stdio};

/// Spawn a background extraction process for the given command history entry.
///
/// Launches `rt pipeline extract <cmd_id>` as a detached child process.
/// Returns an error if the current executable cannot be resolved or the
/// subprocess fails to start.
pub fn spawn_extraction(cmd_id: i64) -> Result<(), Error> {
    let rt_bin = std::env::current_exe()?;
    Command::new(rt_bin)
        .arg("pipeline")
        .arg("extract")
        .arg(cmd_id.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| Error::Io(e))?;
    Ok(())
}
