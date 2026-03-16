use std::sync::Arc;

use crate::agent::knowledge::ScanSession;
use crate::agent::llm::{ChatMessage, ChatResponse, LlmProvider, collect_chat_response};
use crate::error::Error;

fn build_query_prompt(session: &ScanSession) -> String {
    let kb_summary = session.knowledge.to_context_summary();
    let findings_json =
        serde_json::to_string_pretty(&session.findings).unwrap_or_else(|_| "[]".to_string());

    format!(
        r#"You are a security scan analyst. You have access to the complete results of a penetration test scan.
Answer the user's questions about the scan results accurately and concisely.

## Scan Metadata
- Session ID: {id}
- Target: {target}
- Hosts: {hosts}
- Total turns used: {turns}
- Status: {status:?}
- Scan date: {date}
{kb_summary}

## Findings ({count} total)
{findings_json}

Answer the user's questions based on this data. If the data doesn't contain enough information to answer, say so clearly.
Be specific and reference actual findings, hosts, credentials, or flags when relevant."#,
        id = session.id,
        target = session.target_url.as_deref().unwrap_or("N/A"),
        hosts = session.target_hosts.join(", "),
        turns = session.total_turns_used,
        status = session.status,
        date = session.created_at,
        count = session.findings.len(),
    )
}

pub async fn query_oneshot(
    session: &ScanSession,
    provider: &Arc<dyn LlmProvider>,
    question: &str,
) -> Result<String, Error> {
    let prompt = format!("{}\n\nQuestion: {}", build_query_prompt(session), question);
    let messages = vec![ChatMessage::user(prompt)];

    let stream = provider
        .chat(&messages, &[], None)
        .await
        .map_err(Error::from)?;
    match collect_chat_response(stream).await {
        Ok(ChatResponse::Text(t)) => Ok(t),
        Ok(_) => Err(Error::Parse("unexpected tool_use in query".into())),
        Err(e) => Err(Error::from(e)),
    }
}

pub async fn query_repl(
    session: &ScanSession,
    provider: &Arc<dyn LlmProvider>,
) -> Result<(), Error> {
    use std::io::{BufRead, Write};

    let system_context = build_query_prompt(session);

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    println!(
        "Redtrail Query REPL — Session {} ({})",
        &session.id[..8.min(session.id.len())],
        session.target_url.as_deref().unwrap_or("N/A")
    );
    println!("Type your questions. Press Ctrl+D or type 'exit' to quit.\n");

    let mut history = String::new();

    loop {
        print!("redtrail> ");
        stdout.flush().map_err(Error::Io)?;

        let mut line = String::new();
        let bytes_read = stdin.lock().read_line(&mut line).map_err(Error::Io)?;

        if bytes_read == 0 {
            println!();
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "exit" || trimmed == "quit" {
            break;
        }

        let prompt = format!("{system_context}\n\n{history}User: {trimmed}");
        let messages = vec![ChatMessage::user(prompt)];

        let stream = match provider.chat(&messages, &[], None).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error: {e}");
                continue;
            }
        };
        match collect_chat_response(stream).await {
            Ok(ChatResponse::Text(response)) => {
                println!("\n{response}\n");
                history.push_str(&format!("User: {trimmed}\nAssistant: {response}\n\n"));
            }
            Ok(_) => {
                eprintln!("Unexpected response type");
            }
            Err(e) => {
                eprintln!("Error: {e}");
            }
        }
    }

    Ok(())
}
