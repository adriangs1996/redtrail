use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CompletionResult {
    pub value: String,
    pub source: String,
}

struct BuiltinDef {
    subcommands: Vec<&'static str>,
}

pub struct CompletionEngine {
    builtins: HashMap<&'static str, BuiltinDef>,
}

impl Default for CompletionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CompletionEngine {
    pub fn new() -> Self {
        let mut builtins = HashMap::new();
        builtins.insert("session", BuiltinDef {
            subcommands: vec!["new", "list", "switch", "delete", "save", "export", "info", "clone"],
        });
        builtins.insert("sql", BuiltinDef { subcommands: vec![] });
        builtins.insert("tools", BuiltinDef {
            subcommands: vec!["list", "enable", "disable"],
        });
        builtins.insert("env", BuiltinDef {
            subcommands: vec!["list", "set", "unset"],
        });
        builtins.insert("provider", BuiltinDef {
            subcommands: vec!["list", "set"],
        });
        builtins.insert("target", BuiltinDef {
            subcommands: vec!["set", "list", "add"],
        });
        builtins.insert("jobs", BuiltinDef { subcommands: vec![] });
        builtins.insert("help", BuiltinDef { subcommands: vec![] });
        builtins.insert("ask", BuiltinDef { subcommands: vec![] });
        builtins.insert("chat", BuiltinDef { subcommands: vec![] });

        Self { builtins }
    }

    pub fn complete(&self, input: &str, _session_id: Option<&str>) -> Vec<CompletionResult> {
        let trimmed = input.trim();
        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        match parts.len() {
            0 => {
                self.builtins.keys().map(|name| CompletionResult {
                    value: name.to_string(),
                    source: "builtin".to_string(),
                }).collect()
            }
            1 => {
                let prefix = parts[0];
                self.builtins.keys()
                    .filter(|name| name.starts_with(prefix))
                    .map(|name| CompletionResult {
                        value: name.to_string(),
                        source: "builtin".to_string(),
                    })
                    .collect()
            }
            _ => {
                let cmd = parts[0];
                let sub_prefix = parts.last().unwrap_or(&"");
                if let Some(def) = self.builtins.get(cmd) {
                    def.subcommands.iter()
                        .filter(|sc| sc.starts_with(sub_prefix))
                        .map(|sc| CompletionResult {
                            value: sc.to_string(),
                            source: "builtin".to_string(),
                        })
                        .collect()
                } else {
                    vec![]
                }
            }
        }
    }
}
