use crate::core::db;
use crate::error::Error;
use rusqlite::Connection;

pub fn list(conn: &Connection) -> Result<(), Error> {
    let sessions = db::list_sessions(conn, 50)?;

    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    for s in &sessions {
        let cwd = s.cwd_initial.as_deref().unwrap_or("-");
        let shell = s.shell.as_deref().unwrap_or("?");
        println!(
            "{}\t{}\t{}\tcmds:{}",
            s.id, cwd, shell, s.command_count
        );
    }

    Ok(())
}

pub fn detail(conn: &Connection, session_id: &str) -> Result<(), Error> {
    let session = db::get_session(conn, session_id)?;

    println!("Session:  {}", session.id);
    if let Some(cwd) = &session.cwd_initial {
        println!("CWD:      {cwd}");
    }
    if let Some(shell) = &session.shell {
        println!("Shell:    {shell}");
    }
    println!("Source:   {}", session.source);
    println!("Commands: {}", session.command_count);
    println!("---");

    let commands = db::get_commands(conn, &db::CommandFilter {
        session_id: Some(session_id),
        limit: Some(500),
        ..Default::default()
    })?;

    for c in commands.iter().rev() {
        let exit = c.exit_code.map(|e| e.to_string()).unwrap_or_else(|| "?".into());
        println!("[{}] {}", exit, c.command_raw);
    }

    Ok(())
}
