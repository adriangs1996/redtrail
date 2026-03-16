pub mod commands;
pub mod knowledge;

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{RwLock, mpsc};

use crate::agent::llm::LlmProvider;
use crate::agent::strategist::Strategist;
use crate::agent::tools::ToolRegistry;
use crate::db::Db;
use crate::tui::channel::{DriverCommand, DriverEvent, InputMode};
use crate::types::Target;
use knowledge::KnowledgeBase;

pub struct Context {
    pub knowledge: Arc<RwLock<KnowledgeBase>>,
    pub provider: Arc<dyn LlmProvider>,
    pub strategist: Strategist,
    pub target: Target,
    pub tools: Arc<ToolRegistry>,
    pub db: Option<Db>,
    pub event_tx: mpsc::Sender<DriverEvent>,
}

#[async_trait]
pub trait Handle {
    async fn handle(&self, ctx: &mut Context);
}

pub struct Backend {
    ctx: Context,
    command_rx: mpsc::Receiver<DriverCommand>,
    mode: InputMode,
}

impl Backend {
    pub fn new(
        target: Target,
        event_tx: mpsc::Sender<DriverEvent>,
        command_rx: mpsc::Receiver<DriverCommand>,
        db: Option<Db>,
        provider: Arc<dyn LlmProvider>,
        tools: Arc<ToolRegistry>,
        knowledge: Arc<RwLock<KnowledgeBase>>,
    ) -> Self {
        let strategist = Strategist::new(provider.clone());

        Self {
            ctx: Context {
                knowledge,
                provider,
                strategist,
                target,
                tools,
                db,
                event_tx,
            },
            command_rx,
            mode: InputMode::Chat,
        }
    }

    pub async fn run(&mut self) {
        while let Some(cmd) = self.command_rx.recv().await {
            match cmd {
                DriverCommand::Quit => break,
                DriverCommand::Input(text) => {
                    let trimmed = text.trim();
                    match trimmed.to_lowercase().as_str() {
                        "/sh" | "/terminal" => self.switch_mode(InputMode::Terminal).await,
                        "/chat" => self.switch_mode(InputMode::Chat).await,
                        "/tools" => commands::ListTools.handle(&mut self.ctx).await,
                        _ => self.dispatch(text).await,
                    }
                }
            }
        }
    }

    async fn switch_mode(&mut self, mode: InputMode) {
        self.mode = mode;
        let _ = self.ctx.event_tx.send(DriverEvent::ModeChanged(mode)).await;
    }

    async fn dispatch(&mut self, text: String) {
        match self.mode {
            InputMode::Chat => {
                commands::ProcessInput::new(text)
                    .handle(&mut self.ctx)
                    .await;
            }
            InputMode::Terminal => {
                commands::RunShell::new(text).handle(&mut self.ctx).await;
            }
        }
    }
}
