use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use tokio::process::Command;
use tokio::sync::RwLock;

use crate::agent::knowledge::{CommandRecord, CommandSource, KnowledgeBase};
use crate::agent::tools::{RegisteredTool, ToolDef, ToolHandler};

#[derive(Deserialize, JsonSchema)]
pub struct RunCommandInput {
    /// The shell command to execute (passed to sh -c)
    pub command: String,
    /// Working directory (defaults to current dir)
    pub cwd: Option<String>,
    /// Timeout in seconds (defaults to 120)
    pub timeout: Option<u64>,
}

pub fn run_command(kb: Arc<RwLock<KnowledgeBase>>) -> RegisteredTool {
    use schemars::schema_for;

    let schema = schema_for!(RunCommandInput);
    let mut input_schema = serde_json::to_value(schema).unwrap();
    if let Some(obj) = input_schema.as_object_mut() {
        obj.remove("$schema");
        obj.remove("title");
    }
    crate::agent::tools::simplify_schema(&mut input_schema);

    let def = ToolDef {
        name: "run_command".to_string(),
        description: "Execute a shell command and return its stdout/stderr".to_string(),
        input_schema,
    };

    let handler: ToolHandler = Box::new(move |raw: serde_json::Value| {
        let kb = kb.clone();
        Box::pin(async move {
            let input: RunCommandInput =
                serde_json::from_value(raw).map_err(|e| format!("invalid input: {e}"))?;

            let timeout_secs = input.timeout.unwrap_or(120);

            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(&input.command);

            if let Some(ref cwd) = input.cwd {
                cmd.current_dir(cwd);
            }

            let result =
                tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output())
                    .await;

            match result {
                Err(_) => Ok(serde_json::json!({
                    "error": format!("command timed out after {timeout_secs}s"),
                    "command": input.command,
                })),
                Ok(Err(e)) => Ok(serde_json::json!({
                    "error": format!("failed to spawn: {e}"),
                    "command": input.command,
                })),
                Ok(Ok(output)) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();

                    kb.write().await.add_command(CommandRecord {
                        command: input.command.clone(),
                        exit_code: output.status.code(),
                        stdout: stdout.clone(),
                        stderr: stderr.clone(),
                        source: CommandSource::Tool,
                        timestamp: ts,
                    });

                    Ok(serde_json::json!({
                        "exit_code": output.status.code(),
                        "stdout": stdout,
                        "stderr": stderr,
                    }))
                }
            }
        })
    });

    RegisteredTool { def, handler }
}
