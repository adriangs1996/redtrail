use async_trait::async_trait;

use crate::{
    backend::{Context, Handle},
    tui::DriverEvent,
};

pub struct ListTools;

#[async_trait]
impl Handle for ListTools {
    async fn handle(&self, ctx: &mut Context) {
        let defs = ctx.tools.definitions();

        if defs.is_empty() {
            let _ = ctx
                .event_tx
                .send(DriverEvent::Token("No tools registered.\n".into()))
                .await;
        } else {
            let _ = ctx
                .event_tx
                .send(DriverEvent::Token(format!(
                    "Available tools ({}):\n",
                    defs.len()
                )))
                .await;
            for def in &defs {
                let _ = ctx
                    .event_tx
                    .send(DriverEvent::Token(format!(
                        "  {} — {}\n",
                        def.name, def.description
                    )))
                    .await;
            }
        }

        let _ = ctx.event_tx.send(DriverEvent::Done).await;
    }
}
