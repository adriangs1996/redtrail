use crate::core::capture;
use crate::core::db;
use crate::error::Error;
use rusqlite::Connection;

pub struct CaptureArgs<'a> {
    pub session_id: &'a str,
    pub command: &'a str,
    pub cwd: Option<&'a str>,
    pub exit_code: Option<i32>,
    pub ts_start: i64,
    pub ts_end: Option<i64>,
    pub shell: Option<&'a str>,
    pub hostname: Option<&'a str>,
}

pub fn run(conn: &Connection, args: &CaptureArgs) -> Result<(), Error> {
    let binary = capture::extract_binary(args.command);

    // Check blacklist
    let blacklist = capture::default_blacklist();
    if capture::is_blacklisted(binary, &blacklist) {
        return Ok(());
    }

    db::insert_command_redacted(
        conn,
        &db::NewCommand {
            session_id: args.session_id,
            command_raw: args.command,
            command_binary: if binary.is_empty() { None } else { Some(binary) },
            cwd: args.cwd,
            exit_code: args.exit_code,
            timestamp_start: args.ts_start,
            timestamp_end: args.ts_end,
            shell: args.shell,
            hostname: args.hostname,
            source: "human",
            ..Default::default()
        },
    )?;

    Ok(())
}
