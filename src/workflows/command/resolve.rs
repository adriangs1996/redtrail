const BUILTINS: &[&str] = &[
    "session", "sql", "tools", "env", "provider", "target", "jobs", "help", "ask", "chat", "clear",
];

#[derive(Debug, Clone)]
pub enum ResolvedCommand {
    Builtin { name: String, args: Vec<String> },
    Shell { raw: String, background: bool },
}

pub fn resolve(input: &str) -> ResolvedCommand {
    let trimmed = input.trim();
    let first_word = trimmed.split_whitespace().next().unwrap_or("");

    if BUILTINS.contains(&first_word) {
        let args: Vec<String> = trimmed
            .split_whitespace()
            .skip(1)
            .map(String::from)
            .collect();
        return ResolvedCommand::Builtin {
            name: first_word.to_string(),
            args,
        };
    }

    let background = trimmed.ends_with('&');
    let raw = if background {
        trimmed.trim_end_matches('&').trim().to_string()
    } else {
        trimmed.to_string()
    };

    ResolvedCommand::Shell { raw, background }
}
