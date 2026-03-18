use clap::Subcommand;
use crate::db::Db;
use crate::error::Error;
use super::resolve_session;

#[derive(Subcommand)]
pub enum HypothesisCommands {
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Create {
        statement: String,
        #[arg(long)]
        category: String,
        #[arg(long, default_value = "medium")]
        priority: String,
        #[arg(long, default_value_t = 0.5)]
        confidence: f64,
        #[arg(long)]
        component: Option<String>,
    },
    Update {
        id: i64,
        #[arg(long)]
        status: String,
    },
    Show {
        id: i64,
        #[arg(long)]
        json: bool,
    },
}

pub fn run(command: HypothesisCommands) -> Result<(), Error> {
    let (db, session_id) = resolve_session()?;
    match command {
        HypothesisCommands::List { status, json } => {
            let rows = db.list_hypotheses(&session_id, status.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else {
                for h in &rows {
                    println!("[{}] {} ({}) — {} priority={} conf={:.1}",
                        h["id"], h["statement"].as_str().unwrap_or(""),
                        h["category"].as_str().unwrap_or(""),
                        h["status"].as_str().unwrap_or(""),
                        h["priority"].as_str().unwrap_or(""),
                        h["confidence"].as_f64().unwrap_or(0.0),
                    );
                }
            }
        }
        HypothesisCommands::Create { statement, category, priority, confidence, component } => {
            let id = db.create_hypothesis(
                &session_id,
                &statement,
                &category,
                &priority,
                confidence,
                component.as_deref(),
            )?;
            println!("hypothesis created: {id}");
        }
        HypothesisCommands::Update { id, status } => {
            db.update_hypothesis(id, &status)?;
            println!("hypothesis {id} updated to {status}");
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
                println!("  confidence:{:.1}", h["confidence"].as_f64().unwrap_or(0.0));
                if let Some(tc) = h["target_component"].as_str() {
                    println!("  component: {tc}");
                }
                let evidence = h["evidence"].as_array().unwrap();
                if !evidence.is_empty() {
                    println!("  evidence ({}):", evidence.len());
                    for e in evidence {
                        println!("    [{}] {} ({})", e["id"], e["finding"].as_str().unwrap_or(""), e["severity"].as_str().unwrap_or(""));
                    }
                }
            }
        }
    }
    Ok(())
}
