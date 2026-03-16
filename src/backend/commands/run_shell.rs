use async_trait::async_trait;
use tokio::process::Command;

use crate::{
    agent::knowledge::{CommandRecord, CommandSource},
    backend::{Context, Handle},
    tui::DriverEvent,
};

pub struct RunShell {
    input: String,
}

impl RunShell {
    pub fn new(input: String) -> Self {
        Self { input }
    }
}

#[async_trait]
impl Handle for RunShell {
    async fn handle(&self, ctx: &mut Context) {
        let trimmed = self.input.trim();
        if trimmed.is_empty() {
            let _ = ctx.event_tx.send(DriverEvent::Done).await;
            return;
        }

        let output = match Command::new("sh").arg("-c").arg(trimmed).output().await {
            Ok(o) => o,
            Err(e) => {
                let _ = ctx.event_tx.send(DriverEvent::Error(e.to_string())).await;
                let _ = ctx.event_tx.send(DriverEvent::Done).await;
                return;
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        if !stdout.is_empty() {
            let _ = ctx.event_tx.send(DriverEvent::Token(stdout.clone())).await;
        }
        if !stderr.is_empty() {
            let _ = ctx.event_tx.send(DriverEvent::Token(stderr.clone())).await;
        }

        if !output.status.success()
            && let Some(code) = output.status.code() {
                let _ = ctx
                    .event_tx
                    .send(DriverEvent::Token(format!("\n[exit code: {code}]\n")))
                    .await;
            }

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        ctx.knowledge.write().await.add_command(CommandRecord {
            command: trimmed.to_string(),
            exit_code: output.status.code(),
            stdout,
            stderr,
            source: CommandSource::Terminal,
            timestamp: ts,
        });

        let _ = ctx.event_tx.send(DriverEvent::Done).await;
    }
}
