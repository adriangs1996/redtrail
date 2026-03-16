use std::sync::Arc;

use tokio::sync::mpsc;

use crate::agent::knowledge::KnowledgeBase;
use crate::agent::llm::LlmProvider;
use crate::agent::strategist::Strategist;
use crate::agent::tools::ToolRegistry;
use crate::db::Db;
use crate::error::Error;
use crate::tui::channel::{DriverCommand, DriverEvent};
use crate::types::{Finding, Target};

pub struct Driver {
    pub knowledge: KnowledgeBase,
    provider: Arc<dyn LlmProvider>,
    strategist: Strategist,
    target: Target,
    all_findings: Vec<Finding>,
    event_tx: mpsc::Sender<DriverEvent>,
    cmd_rx: mpsc::Receiver<DriverCommand>,
    db: Option<Db>,
    session_id: String,
    tools: Arc<ToolRegistry>,
}

impl Driver {
    pub fn new(
        target: Target,
        event_tx: mpsc::Sender<DriverEvent>,
        cmd_rx: mpsc::Receiver<DriverCommand>,
        db: Option<Db>,
        provider: Arc<dyn LlmProvider>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        let strategist = Strategist::new(provider.clone());

        Self {
            knowledge: KnowledgeBase::new(),
            provider,
            strategist,
            target,
            all_findings: Vec::new(),
            event_tx,
            cmd_rx,
            db,
            session_id: uuid::Uuid::new_v4().to_string(),
            tools,
        }
    }

    pub async fn run(&mut self) -> Result<Vec<Finding>, Error> {
        while let Some(cmd) = self.cmd_rx.recv().await {
            match cmd {
                DriverCommand::Quit => break,
                DriverCommand::Input(text) => {
                    self.process_input(&text).await;
                }
            }
        }

        Ok(self.all_findings.clone())
    }

    async fn process_input(&mut self, input: &str) {
        // TODO: placeholder — wire up actual processing logic here.
        // For now, echo back the input as a demonstration.
        let _ = self
            .event_tx
            .send(DriverEvent::Token(format!("You said: {input}")))
            .await;
        let _ = self.event_tx.send(DriverEvent::Done).await;
    }
}
