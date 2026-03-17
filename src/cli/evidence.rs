use clap::Subcommand;
use crate::error::Error;
use super::resolve_session;

#[derive(Subcommand)]
pub enum EvidenceCommands {
    Add {
        #[arg(long)]
        finding: String,
        #[arg(long)]
        hypothesis: Option<i64>,
        #[arg(long, default_value = "info")]
        severity: String,
        #[arg(long)]
        poc: Option<String>,
    },
    List {
        #[arg(long)]
        hypothesis: Option<i64>,
        #[arg(long)]
        json: bool,
    },
    Export {
        #[arg(long)]
        json: bool,
    },
}

pub fn run(command: EvidenceCommands) -> Result<(), Error> {
    let (db, session_id) = resolve_session()?;
    match command {
        EvidenceCommands::Add { finding, hypothesis, severity, poc } => {
            let id = db.create_evidence(&session_id, hypothesis, &finding, &severity, poc.as_deref())?;
            println!("evidence added: {id}");
        }
        EvidenceCommands::List { hypothesis, json } => {
            let rows = db.list_evidence(&session_id, hypothesis)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else {
                for e in &rows {
                    let hyp = match e["hypothesis_id"].as_i64() {
                        Some(h) => format!("hyp={h}"),
                        None => "unlinked".to_string(),
                    };
                    println!("[{}] {} ({}) {}", e["id"], e["finding"].as_str().unwrap_or(""), e["severity"].as_str().unwrap_or(""), hyp);
                }
            }
        }
        EvidenceCommands::Export { json } => {
            let rows = db.export_evidence(&session_id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else {
                for group in &rows {
                    let hyp_id = &group["hypothesis_id"];
                    let statement = group["statement"].as_str().unwrap_or("(unlinked)");
                    println!("hypothesis {hyp_id}: {statement}");
                    if let Some(evidence) = group["evidence"].as_array() {
                        for e in evidence {
                            println!("  [{}] {} ({})", e["id"], e["finding"].as_str().unwrap_or(""), e["severity"].as_str().unwrap_or(""));
                            if let Some(poc) = e["poc"].as_str() {
                                println!("    poc: {poc}");
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
