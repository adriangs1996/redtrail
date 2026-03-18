use clap::Subcommand;
use crate::db::{KnowledgeBase, Hypotheses, SessionOps};
use crate::error::Error;

#[derive(Subcommand)]
pub enum ReportCommands {
    Generate {
        #[arg(long, default_value = "md")]
        format: String,
        #[arg(long)]
        output: Option<String>,
    },
}

pub fn run(db: &(impl KnowledgeBase + Hypotheses + SessionOps), session_id: &str, command: ReportCommands) -> Result<(), Error> {
    match command {
        ReportCommands::Generate { format: _, output } => {
            let md = build_report(db, session_id)?;
            if let Some(path) = output {
                std::fs::write(&path, &md)?;
                println!("report written to {path}");
            } else {
                print!("{md}");
            }
            Ok(())
        }
    }
}

fn format_finding(ev: &serde_json::Value, hypotheses: &[serde_json::Value]) -> String {
    let finding = ev["finding"].as_str().unwrap_or("");
    let poc = ev["poc"].as_str().unwrap_or("");
    let hyp_stmt = ev["hypothesis_id"].as_i64().and_then(|id| {
        hypotheses.iter().find(|h| h["id"].as_i64() == Some(id))
            .and_then(|h| h["statement"].as_str())
    });
    match (hyp_stmt, poc.is_empty()) {
        (Some(stmt), false) => format!("- {finding} (Hypothesis: {stmt}, PoC: `{poc}`)"),
        (Some(stmt), true) => format!("- {finding} (Hypothesis: {stmt})"),
        (None, false) => format!("- {finding} (PoC: `{poc}`)"),
        (None, true) => format!("- {finding}"),
    }
}

fn build_report(db: &(impl KnowledgeBase + Hypotheses + SessionOps), session_id: &str) -> Result<String, Error> {
    let summary = db.status_summary(session_id)?;
    let hosts = db.list_hosts(session_id)?;
    let ports = db.list_ports(session_id, None)?;
    let creds = db.list_credentials(session_id)?;
    let flags = db.list_flags(session_id)?;
    let hypotheses = db.list_hypotheses(session_id, None)?;
    let evidence = db.list_evidence(session_id, None)?;
    let history = db.list_history(session_id, 1000)?;

    let session_name = summary["session_name"].as_str().unwrap_or("unknown");
    let target = summary["target"].as_str().unwrap_or("N/A");
    let goal = summary["goal"].as_str().unwrap_or("general");
    let phase = summary["phase"].as_str().unwrap_or("L0");
    let noise_budget = summary["noise_budget"].as_f64().unwrap_or(1.0);

    let hosts_count = summary["hosts"].as_i64().unwrap_or(0);
    let ports_count = summary["ports"].as_i64().unwrap_or(0);
    let creds_count = summary["creds"].as_i64().unwrap_or(0);
    let flags_count = summary["flags"].as_i64().unwrap_or(0);
    let hyp_confirmed = summary["hypotheses_confirmed"].as_i64().unwrap_or(0);
    let hyp_refuted = summary["hypotheses_refuted"].as_i64().unwrap_or(0);
    let hyp_total = hypotheses.len() as i64;

    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let mut md = String::new();

    md.push_str(&format!("# Penetration Test Report: {session_name}\n\n"));
    md.push_str(&format!("**Target:** {target}\n"));
    md.push_str(&format!("**Date:** {date}\n"));
    md.push_str(&format!("**Goal:** {goal}\n"));
    md.push_str(&format!("**Phase reached:** {phase}\n\n"));

    md.push_str("## Executive Summary\n\n");
    md.push_str(&format!(
        "{hosts_count} hosts discovered, {ports_count} ports identified, \
{creds_count} credentials found, {flags_count} flags captured.\n\
{hyp_confirmed} vulnerabilities confirmed out of {hyp_total} hypotheses tested.\n\n"
    ));

    md.push_str("## Findings\n\n");
    for (heading, severities) in &[
        ("Critical", vec!["critical"]),
        ("High", vec!["high"]),
        ("Medium", vec!["medium"]),
        ("Low / Info", vec!["low", "info"]),
    ] {
        let bucket: Vec<_> = evidence.iter()
            .filter(|e| severities.contains(&e["severity"].as_str().unwrap_or("info")))
            .collect();
        if !bucket.is_empty() {
            md.push_str(&format!("### {heading}\n\n"));
            for ev in bucket {
                md.push_str(&format_finding(ev, &hypotheses));
                md.push('\n');
            }
            md.push('\n');
        }
    }

    md.push_str("## Discovered Hosts\n\n");
    md.push_str("| IP | Hostname | OS | Ports |\n");
    md.push_str("|---|---|---|---|\n");
    for host in &hosts {
        let ip = host["ip"].as_str().unwrap_or("");
        let hostname = host["hostname"].as_str().unwrap_or("-");
        let os = host["os"].as_str().unwrap_or("-");
        let host_ports: Vec<String> = ports.iter()
            .filter(|p| p["ip"].as_str() == Some(ip))
            .map(|p| {
                let port = p["port"].as_i64().unwrap_or(0);
                let svc = p["service"].as_str().unwrap_or("");
                if svc.is_empty() { format!("{port}") } else { format!("{port}/{svc}") }
            })
            .collect();
        let ports_str = if host_ports.is_empty() { "-".to_string() } else { host_ports.join(", ") };
        md.push_str(&format!("| {ip} | {hostname} | {os} | {ports_str} |\n"));
    }
    md.push('\n');

    if !flags.is_empty() {
        md.push_str("## Flags\n\n");
        md.push_str("| Value | Source | Captured At |\n");
        md.push_str("|---|---|---|\n");
        for f in &flags {
            let value = f["value"].as_str().unwrap_or("");
            let source = f["source"].as_str().unwrap_or("-");
            let captured_at = f["captured_at"].as_str().unwrap_or("-");
            md.push_str(&format!("| {value} | {source} | {captured_at} |\n"));
        }
        md.push('\n');
    }

    if !creds.is_empty() {
        md.push_str("## Credentials\n\n");
        md.push_str("| Username | Password | Service | Host | Source |\n");
        md.push_str("|---|---|---|---|---|\n");
        for c in &creds {
            let username = c["username"].as_str().unwrap_or("");
            let password = c["password"].as_str().unwrap_or("-");
            let service = c["service"].as_str().unwrap_or("-");
            let host = c["host"].as_str().unwrap_or("-");
            let source = c["source"].as_str().unwrap_or("-");
            md.push_str(&format!("| {username} | {password} | {service} | {host} | {source} |\n"));
        }
        md.push('\n');
    }

    if !history.is_empty() {
        md.push_str("## Attack Timeline\n\n");
        md.push_str("| Time | Command | Exit | Duration |\n");
        md.push_str("|---|---|---|---|\n");
        let mut timeline: Vec<_> = history.iter().collect();
        timeline.sort_by_key(|h| h["id"].as_i64().unwrap_or(0));
        for h in timeline {
            let started_at = h["started_at"].as_str().unwrap_or("-");
            let command = h["command"].as_str().unwrap_or("");
            let exit_code = h["exit_code"].as_i64().map(|c| c.to_string()).unwrap_or("-".to_string());
            let duration = h["duration_ms"].as_i64()
                .map(|ms| format!("{ms}ms"))
                .unwrap_or("-".to_string());
            md.push_str(&format!("| {started_at} | `{command}` | {exit_code} | {duration} |\n"));
        }
        md.push('\n');
    }

    if !hypotheses.is_empty() {
        md.push_str("## Hypotheses\n\n");
        md.push_str("| ID | Statement | Category | Status | Priority |\n");
        md.push_str("|---|---|---|---|---|\n");
        for h in &hypotheses {
            let id = h["id"].as_i64().unwrap_or(0);
            let stmt = h["statement"].as_str().unwrap_or("");
            let cat = h["category"].as_str().unwrap_or("");
            let status = h["status"].as_str().unwrap_or("");
            let priority = h["priority"].as_str().unwrap_or("");
            md.push_str(&format!("| {id} | {stmt} | {cat} | {status} | {priority} |\n"));
        }
        md.push('\n');
    }

    md.push_str("## Methodology\n\n");
    md.push_str("Redtrail deductive methodology (L0-L4):\n\n");
    let phase_num: i32 = phase.trim_start_matches('L').parse().unwrap_or(0);
    for (i, label) in ["Reconnaissance", "Hypothesis Generation", "Probing", "Exploitation"].iter().enumerate() {
        let status = if i as i32 <= phase_num { "complete" } else { "pending" };
        md.push_str(&format!("- L{i} {label}: {status}\n"));
    }
    md.push('\n');

    md.push_str("## Metrics\n\n");
    md.push_str(&format!("- Total commands: {}\n", history.len()));
    md.push_str(&format!("- Hypotheses: {hyp_total} generated, {hyp_confirmed} confirmed, {hyp_refuted} refuted\n"));
    md.push_str(&format!("- Noise budget remaining: {noise_budget:.2}\n"));

    Ok(md)
}
