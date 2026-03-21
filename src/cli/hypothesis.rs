use crate::db::Hypotheses;
use crate::error::Error;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum HypothesisCommands {
    #[command(about = "List all hypotheses, optionally filtered by status")]
    List {
        #[arg(long, help = "Filter by status (e.g. pending, confirmed, refuted)")]
        status: Option<String>,
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
    #[command(about = "Show full details and linked evidence for a hypothesis")]
    Show {
        #[arg(help = "Hypothesis ID")]
        id: i64,
        #[arg(long, help = "Output as JSON")]
        json: bool,
    },
}

pub fn run(
    db: &impl Hypotheses,
    session_id: &str,
    command: HypothesisCommands,
) -> Result<(), Error> {
    match command {
        HypothesisCommands::List { status, json } => {
            let rows = db.list_hypotheses(session_id, status.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else {
                for h in &rows {
                    println!(
                        "[{}] {} ({}) — {} priority={} conf={:.1}",
                        h["id"],
                        h["statement"].as_str().unwrap_or(""),
                        h["category"].as_str().unwrap_or(""),
                        h["status"].as_str().unwrap_or(""),
                        h["priority"].as_str().unwrap_or(""),
                        h["confidence"].as_f64().unwrap_or(0.0),
                    );
                }
            }
        }
        HypothesisCommands::Show { id, json } => {
            let h = db.get_hypothesis(id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&h).unwrap());
            } else {
                println!("[{}] {}", h["id"], h["statement"].as_str().unwrap_or(""));
                println!("  category:  {}", h["category"].as_str().unwrap_or(""));
                println!("  status:    {}", h["status"].as_str().unwrap_or(""));
                println!("  priority:  {}", h["priority"].as_str().unwrap_or(""));
                println!(
                    "  confidence:{:.1}",
                    h["confidence"].as_f64().unwrap_or(0.0)
                );
                if let Some(tc) = h["target_component"].as_str() {
                    println!("  component: {tc}");
                }
                let evidence = h["evidence"].as_array().unwrap();
                if !evidence.is_empty() {
                    println!("  evidence ({}):", evidence.len());
                    for e in evidence {
                        println!(
                            "    [{}] {} ({})",
                            e["id"],
                            e["finding"].as_str().unwrap_or(""),
                            e["severity"].as_str().unwrap_or("")
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
