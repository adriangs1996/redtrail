use crate::core::capture::MAX_STDOUT_BYTES;
use crate::core::tee;
use crate::error::Error;

pub struct TeeArgs<'a> {
    pub command_id: &'a str,
    pub shell_pid: &'a str,
    pub ctl_fifo: &'a str,
    pub max_bytes: Option<usize>,
}

pub fn run(args: &TeeArgs) -> Result<(), Error> {
    tee::run_tee(&tee::TeeConfig {
        command_id: args.command_id.to_string(),
        shell_pid: args.shell_pid.to_string(),
        ctl_fifo: args.ctl_fifo.to_string(),
        max_bytes: args.max_bytes.unwrap_or(MAX_STDOUT_BYTES),
    })
}
