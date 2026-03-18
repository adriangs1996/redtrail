use std::fs;
use crate::cli;
use crate::db::Db;
use crate::error::Error;

pub fn run(file: &str, tool_override: Option<String>) -> Result<(), Error> {
    let (db, session_id) = cli::resolve_session()?;
    let content = fs::read_to_string(file)?;
    let tool = tool_override.unwrap_or_else(|| detect_tool(&content));
    let filename = std::path::Path::new(file)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(file);

    let cmd_id = db.insert_command(&session_id, &format!("rt ingest {filename}"), Some(&tool))?;
    db.finish_command(cmd_id, 0, 0, &content)?;

    println!("ingested: {filename} (tool: {tool}, {} bytes)", content.len());
    println!("extraction pending — run `rt kb extract {cmd_id}` to extract manually");
    Ok(())
}

fn detect_tool(content: &str) -> String {
    let lower = content.to_lowercase();
    if lower.contains("<nmaprun") || lower.contains("nmap") && lower.contains("<?xml") {
        "nmap".into()
    } else if lower.contains("gobuster") {
        "gobuster".into()
    } else if lower.contains("\"template-id\"") || lower.contains("nuclei") {
        "nuclei".into()
    } else if lower.contains("nikto") {
        "nikto".into()
    } else if lower.contains("feroxbuster") {
        "feroxbuster".into()
    } else if lower.contains("nmap") {
        "nmap".into()
    } else {
        "unknown".into()
    }
}
