use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellCommand {
    pub raw: String,
    pub program: String,
    pub args: Vec<String>,
    pub env_overrides: HashMap<String, String>,
    pub working_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ShellOutputLine {
    pub text: String,
    pub stream: ShellOutputStream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellOutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub lines: Vec<ShellOutputLine>,
    pub exit_code: Option<i32>,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockStatus {
    Running,
    Success(i32),
    Failed(i32),
}

#[derive(Debug, Clone)]
pub struct Block {
    pub id: usize,
    pub command: String,
    pub content: BlockContent,
    pub status: BlockStatus,
    pub collapsed: bool,
    pub started_at: std::time::Instant,
    pub job_id: Option<u32>,
    pub content_scroll: u16,
}

#[derive(Debug, Clone)]
pub struct TableData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone)]
pub enum BlockContent {
    Plain(Vec<ShellOutputLine>),
    Ansi(Vec<ShellOutputLine>),
    Markdown(Vec<ShellOutputLine>),
    Table(TableData),
}

impl BlockContent {
    pub fn line_count(&self) -> usize {
        match self {
            Self::Plain(lines) | Self::Ansi(lines) | Self::Markdown(lines) => lines.len(),
            Self::Table(data) => data.rows.len() + 4,
        }
    }

    pub fn push_line(&mut self, line: ShellOutputLine) {
        match self {
            Self::Plain(lines) | Self::Ansi(lines) | Self::Markdown(lines) => lines.push(line),
            Self::Table(_) => {}
        }
    }

    pub fn push_token(&mut self, token: &str) {
        if let Self::Markdown(lines) = self {
            for (i, part) in token.split('\n').enumerate() {
                if i > 0 {
                    lines.push(ShellOutputLine {
                        text: String::new(),
                        stream: ShellOutputStream::Stdout,
                    });
                }
                if let Some(last) = lines.last_mut() {
                    last.text.push_str(part);
                } else {
                    lines.push(ShellOutputLine {
                        text: part.to_string(),
                        stream: ShellOutputStream::Stdout,
                    });
                }
            }
        }
    }

    pub fn lines_ref(&self) -> &[ShellOutputLine] {
        match self {
            Self::Plain(lines) | Self::Ansi(lines) | Self::Markdown(lines) => lines,
            Self::Table(_) => &[],
        }
    }
}
