use crate::db::CommandLog;
use crate::error::Error;
use std::fs;

pub fn run(
    db: &impl CommandLog,
    session_id: &str,
    file: &str,
    tool_override: Option<String>,
    auto_extract: bool,
) -> Result<(), Error> {
    let content = fs::read_to_string(file)?;
    let tool = tool_override.unwrap_or_else(|| detect_tool(&content));
    let filename = std::path::Path::new(file)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(file);

    let cmd_id = db.insert_command(session_id, &format!("rt ingest {filename}"), Some(&tool))?;
    db.finish_command(cmd_id, 0, 0, &content)?;

    println!(
        "ingested: {filename} (tool: {tool}, {} bytes)",
        content.len()
    );

    if auto_extract {
        crate::spawn::spawn_extraction(cmd_id);
        println!("extraction queued");
    } else {
        println!("extraction skipped (auto_extract disabled)");
    }

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
