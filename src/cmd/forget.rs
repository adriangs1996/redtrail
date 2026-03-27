use crate::core::db;
use crate::error::Error;
use rusqlite::Connection;

pub struct ForgetArgs<'a> {
    pub command: Option<&'a str>,
    pub session: Option<&'a str>,
    pub since: Option<i64>,
}

pub fn run(conn: &Connection, args: &ForgetArgs) -> Result<(), Error> {
    if let Some(id) = args.command {
        db::forget_command(conn, id)?;
        println!("Deleted command {id}");
    } else if let Some(sid) = args.session {
        db::forget_session(conn, sid)?;
        println!("Deleted session {sid} and all its commands");
    } else if let Some(ts) = args.since {
        db::forget_since(conn, ts)?;
        println!("Deleted commands since timestamp {ts}");
    } else {
        eprintln!("specify one of: --command <id>, --session <id>, --since <timestamp>");
        std::process::exit(1);
    }
    Ok(())
}
