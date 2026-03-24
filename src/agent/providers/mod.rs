use std::path::PathBuf;
use std::pin::Pin;
use std::process::Stdio;

use aisdk::core::capabilities::{TextInputSupport, ToolCallSupport};
use aisdk::core::language_model::{
    LanguageModel, LanguageModelOptions, LanguageModelResponse,
    LanguageModelResponseContentType, LanguageModelStreamChunk, LanguageModelStreamChunkType,
};
use aisdk::core::messages::{AssistantMessage, Message};
use aisdk::core::tools::{ToolCallInfo, ToolDetails};
use aisdk::error::{Error, Result};
use aisdk::extensions::Extensions;
use async_trait::async_trait;
use futures::Stream;
use serde::Deserialize;
use tokio::io::AsyncBufReadExt;

type ProviderStream = Pin<Box<dyn Stream<Item = Result<Vec<LanguageModelStreamChunk>>> + Send>>;

fn err(msg: String) -> Error {
    Error::Other(msg)
}

fn extract_prompt(messages: &[Message]) -> String {
    messages.iter().filter_map(|m| match m {
        Message::User(u) => Some(u.content.as_str()),
        Message::System(s) => Some(s.content.as_str()),
        Message::Developer(s) => Some(s.as_str()),
        _ => None,
    }).collect::<Vec<_>>().join("\n")
}

#[derive(Debug, Deserialize)]
struct ClaudeMessage {
    #[serde(rename = "type")]
    msg_type: String,
    message: Option<serde_json::Value>,
    result: Option<String>,
    cost_usd: Option<f64>,
    #[allow(dead_code)]
    duration_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ClaudeCodeProvider {
    pub allowed_tools: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub max_turns: Option<usize>,
}

impl ClaudeCodeProvider {
    pub fn new() -> Self {
        Self {
            allowed_tools: vec!["Bash".into()],
            cwd: None,
            max_turns: None,
        }
    }

    pub fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }

    pub fn with_max_turns(mut self, max_turns: usize) -> Self {
        self.max_turns = Some(max_turns);
        self
    }

    fn build_args(&self, prompt: &str, system: Option<&str>) -> Vec<String> {
        let mut args = vec![
            "--dangerously-skip-permissions".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
        ];

        if let Some(sys) = system {
            args.push("--system-prompt".to_string());
            args.push(sys.to_string());
        }

        for tool in &self.allowed_tools {
            args.push("--allowedTools".to_string());
            args.push(tool.clone());
        }

        if let Some(max) = self.max_turns {
            args.push("--max-turns".to_string());
            args.push(max.to_string());
        }

        args.push("-p".to_string());
        args.push(prompt.to_string());

        args
    }

    fn spawn_claude(&self, args: &[String]) -> Result<tokio::process::Child> {
        let mut cmd = tokio::process::Command::new("claude");
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(ref cwd) = self.cwd {
            cmd.current_dir(cwd);
        }

        cmd.spawn().map_err(|e| err(format!("failed to spawn claude: {e}")))
    }

    fn drain_stderr(child: &mut tokio::process::Child) {
        if let Some(se) = child.stderr.take() {
            let mut reader = tokio::io::BufReader::new(se).lines();
            tokio::spawn(async move {
                while let Some(line) = reader.next_line().await.unwrap_or(None) {
                    eprintln!("[claude-code] {line}");
                }
            });
        }
    }

    fn parse_assistant_contents(message: &serde_json::Value) -> Vec<LanguageModelResponseContentType> {
        let mut contents = Vec::new();
        let Some(items) = message.get("content").and_then(|c| c.as_array()) else {
            return contents;
        };

        for item in items {
            match item.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                        contents.push(LanguageModelResponseContentType::Text(text.to_string()));
                    }
                }
                Some("tool_use") => {
                    let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let input = item.get("input").cloned().unwrap_or(serde_json::Value::Object(Default::default()));
                    contents.push(LanguageModelResponseContentType::ToolCall(ToolCallInfo {
                        tool: ToolDetails { id, name },
                        input,
                        extensions: Extensions::default(),
                    }));
                }
                _ => {}
            }
        }

        contents
    }

    pub(crate) async fn run_claude(&self, prompt: &str, system: Option<&str>) -> Result<(Vec<LanguageModelResponseContentType>, Option<f64>)> {
        let args = self.build_args(prompt, system);
        let mut child = self.spawn_claude(&args)?;

        let stdout = child.stdout.take()
            .ok_or_else(|| err("no stdout".into()))?;
        let mut reader = tokio::io::BufReader::new(stdout).lines();
        Self::drain_stderr(&mut child);

        let mut all_contents: Vec<LanguageModelResponseContentType> = Vec::new();
        let mut cost: Option<f64> = None;

        while let Some(line) = reader.next_line().await
            .map_err(|e| err(format!("stdout read: {e}")))?
        {
            if line.is_empty() {
                continue;
            }

            let msg: ClaudeMessage = serde_json::from_str(&line)
                .map_err(|e| err(format!("parse: {e}")))?;

            match msg.msg_type.as_str() {
                "assistant" => {
                    if let Some(ref message) = msg.message {
                        all_contents.extend(Self::parse_assistant_contents(message));
                    }
                }
                "result" => {
                    if let Some(ref result_text) = msg.result {
                        all_contents.push(LanguageModelResponseContentType::Text(result_text.clone()));
                    }
                    cost = msg.cost_usd;
                }
                _ => {}
            }
        }

        let status = child.wait().await
            .map_err(|e| err(format!("wait: {e}")))?;

        if !status.success() {
            return Err(err(format!("claude exited with {status}")));
        }

        Ok((all_contents, cost))
    }
}

impl TextInputSupport for ClaudeCodeProvider {}
impl ToolCallSupport for ClaudeCodeProvider {}

#[async_trait]
impl LanguageModel for ClaudeCodeProvider {
    fn name(&self) -> String {
        "claude-code".to_string()
    }

    async fn generate_text(
        &mut self,
        options: LanguageModelOptions,
    ) -> Result<LanguageModelResponse> {
        let prompt = extract_prompt(&options.messages());
        let system = options.system.as_deref();
        let (contents, _cost) = self.run_claude(&prompt, system).await?;

        Ok(LanguageModelResponse {
            contents,
            usage: None,
        })
    }

    async fn stream_text(&mut self, options: LanguageModelOptions) -> Result<ProviderStream> {
        let prompt = extract_prompt(&options.messages());
        let system = options.system.clone();
        let args = self.build_args(&prompt, system.as_deref());

        let mut child = self.spawn_claude(&args)?;

        let stdout = child.stdout.take()
            .ok_or_else(|| err("no stdout".into()))?;
        Self::drain_stderr(&mut child);

        let stream = async_stream::try_stream! {
            let mut reader = tokio::io::BufReader::new(stdout).lines();

            yield vec![LanguageModelStreamChunk::Delta(LanguageModelStreamChunkType::Start)];

            while let Some(line) = reader.next_line().await
                .map_err(|e| err(format!("stdout read: {e}")))?
            {
                if line.is_empty() {
                    continue;
                }

                let msg: ClaudeMessage = serde_json::from_str(&line)
                    .map_err(|e| err(format!("parse: {e}")))?;

                match msg.msg_type.as_str() {
                    "assistant" => {
                        if let Some(ref message) = msg.message {
                            let contents = ClaudeCodeProvider::parse_assistant_contents(message);
                            for c in contents {
                                match c {
                                    LanguageModelResponseContentType::Text(text) => {
                                        yield vec![LanguageModelStreamChunk::Delta(
                                            LanguageModelStreamChunkType::Text(text),
                                        )];
                                    }
                                    LanguageModelResponseContentType::ToolCall(ref tc) => {
                                        let json = serde_json::json!({
                                            "id": tc.tool.id,
                                            "name": tc.tool.name,
                                            "input": tc.input,
                                        });
                                        yield vec![LanguageModelStreamChunk::Delta(
                                            LanguageModelStreamChunkType::ToolCall(json.to_string()),
                                        )];
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    "result" => {
                        let text = msg.result.unwrap_or_default();
                        yield vec![LanguageModelStreamChunk::Done(AssistantMessage {
                            content: LanguageModelResponseContentType::Text(text),
                            usage: None,
                        })];
                    }
                    _ => {}
                }
            }

            let _ = child.wait().await;
        };

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
#[cfg(feature = "integration")]
mod tests {
    use super::*;

    #[tokio::test]
    async fn claude_code_generate_text() {
        let mut provider = ClaudeCodeProvider::new();
        let (contents, cost) = provider
            .run_claude("Say hello in one word", None)
            .await
            .unwrap();
        assert!(!contents.is_empty());
        println!("contents: {contents:?}");
        println!("cost: {cost:?}");
    }

    #[tokio::test]
    async fn claude_code_with_bash_tool() {
        let mut provider = ClaudeCodeProvider::new();
        let (contents, _cost) = provider
            .run_claude("Run: echo 'hello from rt'", None)
            .await
            .unwrap();
        let text = contents.iter().filter_map(|c| match c {
            LanguageModelResponseContentType::Text(t) => Some(t.as_str()),
            _ => None,
        }).collect::<Vec<_>>().join("");
        assert!(text.contains("hello from rt"));
    }
}
