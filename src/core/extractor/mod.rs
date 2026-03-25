mod nmap;
mod web_enum;
mod hydra;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub fact_type: String,
    pub key: String,
    pub attributes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub from_key: String,
    pub to_key: String,
    pub relation_type: String,
}

#[derive(Debug, Default)]
pub struct SynthesisResult {
    pub facts: Vec<Fact>,
    pub relations: Vec<Relation>,
}

impl SynthesisResult {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.facts.is_empty() && self.relations.is_empty()
    }
}

pub struct Synthetizer {
    runs_on: fn(&str) -> bool,
    stz: fn(&str, &str) -> SynthesisResult,
}

impl Synthetizer {
    pub const fn new(
        runs_on: fn(&str) -> bool,
        stz: fn(&str, &str) -> SynthesisResult,
    ) -> Self {
        Self { runs_on, stz }
    }
}

inventory::collect!(Synthetizer);

const SKIP_PREFIXES: &[&str] = &[
    "sudo", "proxychains", "proxychains4", "time",
    "strace", "ltrace", "nice", "nohup", "env",
];

pub fn detect_tool(command: &str, tool_hint: Option<&str>) -> Option<String> {
    if let Some(hint) = tool_hint {
        return Some(hint.to_string());
    }
    for token in command.split_whitespace() {
        if token.contains('=') && !token.starts_with('=') {
            continue;
        }
        if SKIP_PREFIXES.contains(&token) {
            continue;
        }
        return Some(token.to_string());
    }
    None
}

pub fn synthetize(command: &str, tool: Option<&str>, output: &str) -> SynthesisResult {
    let tool_name = detect_tool(command, tool);
    let tool_str = match tool_name.as_deref() {
        Some(t) => t,
        None => return SynthesisResult::empty(),
    };
    for entry in inventory::iter::<Synthetizer> {
        if (entry.runs_on)(tool_str) {
            return (entry.stz)(command, output);
        }
    }
    SynthesisResult::empty()
}
