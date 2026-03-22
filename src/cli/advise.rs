use crate::agent;
use crate::agent::strategist::{self, AdviseInput, collect_suggestions};
use crate::error::Error;
use crate::resolve;
use std::sync::{Arc, Mutex};

pub fn run(question: &str) -> Result<(), Error> {
    let cwd = std::env::current_dir()?;
    let ctx = resolve::resolve(&cwd)?;
    let session_id = ctx.session_id.clone();
    let config = ctx.config.clone();

    let model = agent::create_model(&config)?;
    let conn_arc = Arc::new(Mutex::new(ctx.conn));

    let agent = strategist::build_strategist_agent(
        model,
        conn_arc,
        session_id,
        cwd,
    )?;

    let input = AdviseInput {
        question: question.to_string(),
    };
    let prompt = input.to_prompt();

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| Error::Config(format!("tokio runtime: {e}")))?;

    match rt.block_on(agent.run(&prompt)) {
        Ok(response) => {
            if let Some(text) = response.text()
                && !text.is_empty() {
                    println!("{text}");
                }

            let results = response.options.tool_results().unwrap_or_default();
            let suggestions = collect_suggestions(&results);
            if !suggestions.is_empty() {
                println!();
            }
            for s in &suggestions {
                let text = s["text"].as_str().unwrap_or("");
                let priority = s["priority"].as_str().unwrap_or("medium");
                let indicator = match priority {
                    "critical" => "\x1b[1;31m[!!!]\x1b[0m",
                    "high" => "\x1b[1;33m[!!]\x1b[0m",
                    "medium" => "\x1b[1;36m[!]\x1b[0m",
                    _ => "\x1b[2m[·]\x1b[0m",
                };
                println!("{indicator} {text}");
            }
            Ok(())
        }
        Err(e) => Err(Error::Config(format!("strategist agent: {e}"))),
    }
}
