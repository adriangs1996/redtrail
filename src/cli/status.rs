use crate::db::SessionOps;
use crate::error::Error;

pub fn run(db: &impl SessionOps, session_id: &str, json: bool) -> Result<(), Error> {
    let summary = db.status_summary(session_id)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
        return Ok(());
    }

    println!(
        "Session: {}",
        summary["session_name"].as_str().unwrap_or("")
    );
    println!("Target:  {}", summary["target"].as_str().unwrap_or("-"));
    println!("Goal:    {}", summary["goal"].as_str().unwrap_or(""));
    println!("Phase:   {}", summary["phase"].as_str().unwrap_or(""));
    println!();
    println!(
        "Hosts:   {} discovered",
        summary["hosts"].as_i64().unwrap_or(0)
    );
    println!("Ports:   {} open", summary["ports"].as_i64().unwrap_or(0));
    println!("Creds:   {} found", summary["creds"].as_i64().unwrap_or(0));
    println!(
        "Flags:   {} captured",
        summary["flags"].as_i64().unwrap_or(0)
    );
    println!(
        "Hyps:    {} pending, {} confirmed, {} refuted",
        summary["hypotheses_pending"].as_i64().unwrap_or(0),
        summary["hypotheses_confirmed"].as_i64().unwrap_or(0),
        summary["hypotheses_refuted"].as_i64().unwrap_or(0),
    );
    println!(
        "Noise:   {}/1.0",
        summary["noise_budget"].as_f64().unwrap_or(0.0)
    );

    Ok(())
}
