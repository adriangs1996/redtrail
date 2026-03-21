use crate::agent;
use crate::agent::assistant;
use crate::config::Config;
use crate::db;
use crate::error::Error;
use crate::workspace;
use futures::StreamExt;
use std::io::Write;
use std::sync::{Arc, Mutex};

pub fn run(
    message: Option<&str>,
    keep_history: bool,
    clear: bool,
    model_override: Option<&str>,
    skill_override: Option<&str>,
    no_skill: bool,
) -> Result<(), Error> {
    let cwd = std::env::current_dir()?;
    let ws = workspace::find_workspace(&cwd).ok_or(Error::NoWorkspace)?;
    let db_path = workspace::db_path(&ws);
    let db_path_str = db_path
        .to_str()
        .ok_or(Error::Config("invalid db path".into()))?;
    let conn = db::open_connection(db_path_str)?;
    let session_id = db::session::active_session_id(&conn)?;

    if clear {
        let deleted = db::chat::clear(&conn, &session_id)?;
        println!("cleared {deleted} messages");
        return Ok(());
    }

    let message = message.ok_or(Error::Config("no message provided".into()))?;
    let mut config = Config::resolved(&ws)?;
    if let Some(m) = model_override {
        config.general.llm_model = m.to_string();
    }

    let history: Vec<(String, String)> = if keep_history {
        db::chat::load(&conn, &session_id)?
    } else {
        vec![]
    };

    let mut prompt = String::new();
    if !history.is_empty() {
        prompt.push_str("## Conversation History\n");
        for (role, content) in &history {
            prompt.push_str(&format!("[{role}]: {content}\n\n"));
        }
        prompt.push_str("## Current Message\n");
    }
    prompt.push_str(message);

    let model = agent::create_model(&config)?;
    let conn_arc = Arc::new(Mutex::new(conn));

    let agent = assistant::build_assistant_agent(
        model,
        conn_arc.clone(),
        session_id.clone(),
        cwd,
        skill_override,
        no_skill,
    )?;

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| Error::Config(format!("tokio runtime: {e}")))?;

    let final_text = rt.block_on(async {
        stream_response(&agent, &prompt).await
    })?;

    if keep_history {
        let conn = conn_arc.lock().map_err(|e| Error::Config(format!("db lock: {e}")))?;
        db::chat::save(&conn, &session_id, "user", message)?;
        if !final_text.is_empty() {
            db::chat::save(&conn, &session_id, "assistant", &final_text)?;
        }
    }

    Ok(())
}

async fn stream_response<M>(
    agent: &agent::Agent<M>,
    prompt: &str,
) -> Result<String, Error>
where
    M: aisdk::core::language_model::LanguageModel
        + aisdk::core::capabilities::TextInputSupport
        + aisdk::core::capabilities::ToolCallSupport,
{
    let mut response = agent.stream(prompt).await
        .map_err(|e| Error::Config(format!("stream: {e}")))?;

    let mut collected = String::new();

    while let Some(chunk) = response.stream.next().await {
        use aisdk::core::language_model::LanguageModelStreamChunkType;
        match chunk {
            LanguageModelStreamChunkType::Text(text) => {
                print!("{text}");
                std::io::stdout().flush().ok();
                collected.push_str(&text);
            }
            LanguageModelStreamChunkType::Failed(err) => {
                eprintln!("\n[error] {err}");
                return Err(Error::Config(format!("stream failed: {err}")));
            }
            _ => {}
        }
    }

    if !collected.is_empty() && !collected.ends_with('\n') {
        println!();
    }

    let tool_results = response.tool_results().await;
    if let Some(results) = tool_results {
        for res in &results {
            let name = &res.tool.name;
            let output = match &res.output {
                Ok(v) => v.as_str().unwrap_or("").to_string(),
                Err(_) => continue,
            };
            if name == "suggest" {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&output) {
                    let text = parsed["text"].as_str().unwrap_or("");
                    let priority = parsed["priority"].as_str().unwrap_or("medium");
                    let indicator = match priority {
                        "critical" => "\x1b[1;31m[!!!]\x1b[0m",
                        "high" => "\x1b[1;33m[!!]\x1b[0m",
                        "medium" => "\x1b[1;36m[!]\x1b[0m",
                        _ => "\x1b[2m[·]\x1b[0m",
                    };
                    eprintln!("\n{indicator} {text}");
                }
            }
            if name == "respond" {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&output) {
                    if let Some(text) = parsed["text"].as_str() {
                        if collected.is_empty() {
                            render_markdown(text);
                            collected.push_str(text);
                        }
                    }
                }
            }
        }
    }

    Ok(collected)
}

fn render_markdown(text: &str) {
    let mut in_code_block = false;
    for line in text.lines() {
        if line.starts_with("```") {
            in_code_block = !in_code_block;
            if in_code_block {
                println!("\x1b[2m{line}\x1b[0m");
            } else {
                println!("\x1b[2m```\x1b[0m");
            }
            continue;
        }

        if in_code_block {
            println!("\x1b[36m  {line}\x1b[0m");
            continue;
        }

        let rendered = render_inline(line);

        if rendered.starts_with("# ") {
            println!("\x1b[1;4m{}\x1b[0m", &rendered[2..]);
        } else if rendered.starts_with("## ") {
            println!("\x1b[1m{}\x1b[0m", &rendered[3..]);
        } else if rendered.starts_with("### ") {
            println!("\x1b[1m{}\x1b[0m", &rendered[4..]);
        } else if rendered.starts_with("- ") || rendered.starts_with("* ") {
            println!("  \x1b[33m•\x1b[0m {}", &rendered[2..]);
        } else if rendered.starts_with("> ") {
            println!("\x1b[2m│\x1b[0m {}", &rendered[2..]);
        } else {
            println!("{rendered}");
        }
    }
}

fn render_inline(line: &str) -> String {
    let mut result = String::with_capacity(line.len() + 32);
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            if let Some(end) = find_double_star(&chars, i + 2) {
                result.push_str("\x1b[1m");
                for c in &chars[i + 2..end] {
                    result.push(*c);
                }
                result.push_str("\x1b[0m");
                i = end + 2;
                continue;
            }
        }
        if chars[i] == '`' {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '`') {
                result.push_str("\x1b[36m");
                for c in &chars[i + 1..i + 1 + end] {
                    result.push(*c);
                }
                result.push_str("\x1b[0m");
                i = i + 2 + end;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

fn find_double_star(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == '*' && chars[i + 1] == '*' {
            return Some(i);
        }
        i += 1;
    }
    None
}
