use clap::Subcommand;
use crate::db::Hypotheses;
use crate::error::Error;

#[derive(Subcommand)]
pub enum EvidenceCommands {
    #[command(about = "Record a new finding or piece of evidence")]
    Add {
        #[arg(long, help = "Description of the finding")]
        finding: String,
        #[arg(long, help = "Link to a hypothesis ID")]
        hypothesis: Option<i64>,
        #[arg(long, default_value = "info", help = "Severity: info, low, medium, high, critical")]
        severity: String,
        #[arg(long, help = "Proof of concept command or payload")]
        poc: Option<String>,
    },
    #[command(about = "List evidence, optionally filtered by hypothesis")]
    List {
        #[arg(long, help = "Filter by hypothesis ID")]
        hypothesis: Option<i64>,
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Export all evidence grouped by hypothesis")]
    Export {
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
}

pub fn run(db: &impl Hypotheses, session_id: &str, command: EvidenceCommands) -> Result<(), Error> {
    match command {
        EvidenceCommands::Add { finding, hypothesis, severity, poc } => {
            let id = db.create_evidence(session_id, hypothesis, &finding, &severity, poc.as_deref())?;
            println!("evidence added: {id}");
        }
        EvidenceCommands::List { hypothesis, json } => {
            let rows = db.list_evidence(session_id, hypothesis)?;
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
            let rows = db.export_evidence(session_id)?;
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
