use std::process::{Command, Stdio};

pub fn spawn_extraction(cmd_id: i64) {
    let rt_bin = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    let _ = Command::new(rt_bin)
        .arg("pipeline")
        .arg("extract")
        .arg(cmd_id.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn();
}
