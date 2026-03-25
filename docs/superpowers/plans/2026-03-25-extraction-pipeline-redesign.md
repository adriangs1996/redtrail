# Extraction Pipeline Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Strip redtrail to 3 commands (proxy, sql, extract) with polymorphic events/facts/relations schema and deterministic regex extractors.

**Architecture:** Single binary crate with `core/` module (db, extractors, fmt, agent, net) and `cmd/` module (proxy, sql, extract). Extractors self-register via `inventory` crate. `synthesize()` is a pure function — the only public extractor interface.

**Tech Stack:** Rust, SQLite (rusqlite), portable-pty, inventory, sha2, aisdk (claude-code provider), clap, regex, serde_json

**Spec:** `docs/superpowers/specs/2026-03-25-extraction-pipeline-redesign.md`

---

## Task 1: Gut the codebase and set up skeleton

**Files:**
- Delete: all files under `src/cli/`, `src/db/`, `src/agent/`, `src/pipeline.rs`, `src/spawn.rs`, `src/skill_loader.rs`, `src/resolve.rs`
- Delete: all files under `tests/`
- Modify: `Cargo.toml`
- Create: `src/main.rs`, `src/cli.rs`, `src/context.rs`, `src/error.rs`, `src/config.rs`
- Create: `src/core/mod.rs`, `src/core/db.rs`, `src/core/net.rs`
- Create: `src/core/extractors/mod.rs`
- Create: `src/core/fmt/mod.rs`
- Create: `src/core/agent/mod.rs`, `src/core/agent/providers/mod.rs`
- Create: `src/cmd/mod.rs`, `src/cmd/proxy/mod.rs`, `src/cmd/sql/mod.rs`, `src/cmd/extract/mod.rs`

- [ ] **Step 1: Delete old source files**

```bash
rm -rf src/cli src/db src/agent src/pipeline.rs src/spawn.rs src/skill_loader.rs src/resolve.rs
rm -rf tests/*
```

- [ ] **Step 2: Update Cargo.toml**

Replace `Cargo.toml` with:

```toml
[package]
name = "redtrail"
version = "0.2.0"
edition = "2024"
license = "MIT"

[[bin]]
name = "rt"
path = "src/main.rs"

[dependencies]
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
regex = "1"
rusqlite = { version = "0.39", features = ["bundled"] }
portable-pty = "0.9"
aisdk = { version = "0.5", features = ["anthropic", "openaichatcompletions"] }
futures = "0.3"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
async-trait = "0.1.89"
async-stream = "0.3.6"
inventory = "0.3"
sha2 = "0.10"
libc = "0.2"
schemars = "1"
tracing-appender = "0.2"

[dev-dependencies]
tempfile = "3"
```

Note: `schemars` is kept because the aisdk Tool construction requires `schema_for!()`. `libc` is needed for terminal size detection via ioctl. `tracing-appender` is kept for logging.

- [ ] **Step 3: Create error.rs**

```rust
// src/error.rs
use std::fmt;

#[derive(Debug)]
pub enum Error {
    Db(String),
    Config(String),
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Db(e) => write!(f, "database error: {e}"),
            Error::Config(e) => write!(f, "config error: {e}"),
            Error::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
```

- [ ] **Step 4: Create config.rs**

```rust
// src/config.rs
use serde::{Deserialize, Serialize};

fn default_llm_provider() -> String {
    "claude-code".to_string()
}
fn default_llm_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_llm_provider")]
    pub llm_provider: String,
    #[serde(default = "default_llm_model")]
    pub llm_model: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm_provider: default_llm_provider(),
            llm_model: default_llm_model(),
        }
    }
}
```

- [ ] **Step 5: Create core/db.rs with schema and CRUD**

```rust
// src/core/db.rs
use crate::error::Error;
use rusqlite::Connection;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    workspace_path TEXT NOT NULL UNIQUE,
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    command TEXT NOT NULL,
    tool TEXT,
    exit_code INTEGER,
    duration_ms INTEGER,
    output TEXT,
    output_hash TEXT,
    extraction_status TEXT DEFAULT 'stored',
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS facts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    event_id INTEGER NOT NULL REFERENCES events(id),
    fact_type TEXT NOT NULL,
    key TEXT NOT NULL,
    attributes TEXT NOT NULL DEFAULT '{}',
    confidence REAL NOT NULL DEFAULT 1.0,
    source TEXT NOT NULL DEFAULT 'regex',
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    UNIQUE(session_id, key)
);

CREATE TABLE IF NOT EXISTS relations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    from_key TEXT NOT NULL,
    to_key TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    created_at TEXT DEFAULT (datetime('now')),
    UNIQUE(session_id, from_key, to_key, relation_type)
);

CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id);
CREATE INDEX IF NOT EXISTS idx_events_tool ON events(tool);
CREATE INDEX IF NOT EXISTS idx_facts_session_type ON facts(session_id, fact_type);
CREATE INDEX IF NOT EXISTS idx_facts_event ON facts(event_id);
CREATE INDEX IF NOT EXISTS idx_relations_from ON relations(session_id, from_key);
CREATE INDEX IF NOT EXISTS idx_relations_to ON relations(session_id, to_key);
";

pub fn open(path: &str) -> Result<Connection, Error> {
    let conn = Connection::open(path).map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch(SCHEMA)
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection, Error> {
    let conn = Connection::open_in_memory().map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute_batch(SCHEMA)
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn)
}

pub fn ensure_session(conn: &Connection, workspace_path: &str) -> Result<String, Error> {
    // Reuse existing session if one exists for this path
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM sessions WHERE workspace_path = ?1",
            [workspace_path],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        return Ok(id);
    }

    // Create new session: dirname-timestamp
    let dir_name = std::path::Path::new(workspace_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("session");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let id = format!("{dir_name}-{ts}");

    conn.execute(
        "INSERT INTO sessions (id, name, workspace_path) VALUES (?1, ?2, ?3)",
        rusqlite::params![id, dir_name, workspace_path],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    Ok(id)
}

pub fn insert_event(
    conn: &Connection,
    session_id: &str,
    command: &str,
    tool: Option<&str>,
    exit_code: i32,
    duration_ms: i64,
    output: &str,
    output_hash: &str,
) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO events (session_id, command, tool, exit_code, duration_ms, output, output_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![session_id, command, tool, exit_code, duration_ms, output, output_hash],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

pub fn update_extraction_status(
    conn: &Connection,
    event_id: i64,
    status: &str,
) -> Result<(), Error> {
    conn.execute(
        "UPDATE events SET extraction_status = ?1 WHERE id = ?2",
        rusqlite::params![status, event_id],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn insert_fact(
    conn: &Connection,
    session_id: &str,
    event_id: i64,
    fact_type: &str,
    key: &str,
    attributes: &serde_json::Value,
    confidence: f64,
    source: &str,
) -> Result<i64, Error> {
    let attr_str = serde_json::to_string(attributes)
        .map_err(|e| Error::Db(e.to_string()))?;
    conn.execute(
        "INSERT INTO facts (session_id, event_id, fact_type, key, attributes, confidence, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(session_id, key) DO UPDATE SET
            attributes = json_patch(facts.attributes, excluded.attributes),
            confidence = MAX(facts.confidence, excluded.confidence),
            source = CASE WHEN excluded.confidence > facts.confidence THEN excluded.source ELSE facts.source END,
            event_id = excluded.event_id,
            updated_at = datetime('now')",
        rusqlite::params![session_id, event_id, fact_type, key, attr_str, confidence, source],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_relation(
    conn: &Connection,
    session_id: &str,
    from_key: &str,
    to_key: &str,
    relation_type: &str,
) -> Result<(), Error> {
    conn.execute(
        "INSERT OR IGNORE INTO relations (session_id, from_key, to_key, relation_type)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![session_id, from_key, to_key, relation_type],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

/// Store an ExtractionResult (facts + relations) in a single transaction.
pub fn store_extraction(
    conn: &Connection,
    session_id: &str,
    event_id: i64,
    facts: &[crate::core::extractors::Fact],
    relations: &[crate::core::extractors::Relation],
) -> Result<(), Error> {
    let tx = conn.transaction().map_err(|e| Error::Db(e.to_string()))?;

    for fact in facts {
        insert_fact(
            &tx, session_id, event_id,
            &fact.fact_type, &fact.key, &fact.attributes,
            1.0, "regex",
        )?;
    }

    for rel in relations {
        insert_relation(&tx, session_id, &rel.from_key, &rel.to_key, &rel.relation_type)?;
    }

    update_extraction_status(&tx, event_id, "extracted")?;
    tx.commit().map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn global_db_path() -> Result<std::path::PathBuf, Error> {
    let home = std::env::var("HOME")
        .map_err(|_| Error::Config("HOME not set".into()))?;
    let dir = std::path::PathBuf::from(home).join(".redtrail");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("redtrail.db"))
}
```

- [ ] **Step 6: Create core/net.rs**

Copy the existing `src/net.rs` content (ip_to_u32, ip_in_cidr, extract_ips, ip_in_scope) into `src/core/net.rs`. Remove the `#[cfg(test)] mod tests` block — those tests will move to `tests/`.

- [ ] **Step 7: Create core/extractors/mod.rs with synthesize() and detect_tool()**

```rust
// src/core/extractors/mod.rs
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
pub struct ExtractionResult {
    pub facts: Vec<Fact>,
    pub relations: Vec<Relation>,
}

impl ExtractionResult {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.facts.is_empty() && self.relations.is_empty()
    }
}

pub struct ExtractorEntry {
    pub tools: &'static [&'static str],
    /// extract(command, output) -> ExtractionResult
    /// `command` is the full command line string (e.g. "gobuster dir -u http://10.10.10.1 -w wordlist.txt").
    /// Extractors that need target info parse it from the command args.
    pub extract: fn(&str, &str) -> ExtractionResult,
}

impl ExtractorEntry {
    pub const fn new(tools: &'static [&'static str], extract: fn(&str, &str) -> ExtractionResult) -> Self {
        Self { tools, extract }
    }
}

inventory::collect!(ExtractorEntry);

const SKIP_PREFIXES: &[&str] = &[
    "sudo", "proxychains", "proxychains4", "time",
    "strace", "ltrace", "nice", "nohup", "env",
];

pub fn detect_tool(command: &str, tool_hint: Option<&str>) -> Option<String> {
    if let Some(hint) = tool_hint {
        return Some(hint.to_string());
    }
    for token in command.split_whitespace() {
        // Skip env var assignments: KEY=VALUE
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

pub fn synthesize(command: &str, tool: Option<&str>, output: &str) -> ExtractionResult {
    let tool_name = detect_tool(command, tool);
    let tool_str = match tool_name.as_deref() {
        Some(t) => t,
        None => return ExtractionResult::empty(),
    };
    for entry in inventory::iter::<ExtractorEntry> {
        if entry.tools.contains(&tool_str) {
            return (entry.extract)(command, output);
        }
    }
    ExtractionResult::empty()
}
```

- [ ] **Step 8: Create stub extractor files**

Each extractor file needs a minimal stub that compiles. We'll implement them in later tasks.

`src/core/extractors/nmap.rs`:
```rust
use super::{ExtractorEntry, ExtractionResult};

fn extract(_command: &str, _output: &str) -> ExtractionResult {
    ExtractionResult::empty()
}

inventory::submit! {
    ExtractorEntry::new(&["nmap"], extract)
}
```

`src/core/extractors/web_enum.rs`:
```rust
use super::{ExtractorEntry, ExtractionResult};

fn extract(_command: &str, _output: &str) -> ExtractionResult {
    ExtractionResult::empty()
}

inventory::submit! {
    ExtractorEntry::new(&["gobuster", "ffuf", "feroxbuster", "dirb", "wfuzz"], extract)
}
```

`src/core/extractors/hydra.rs`:
```rust
use super::{ExtractorEntry, ExtractionResult};

fn extract(_command: &str, _output: &str) -> ExtractionResult {
    ExtractionResult::empty()
}

inventory::submit! {
    ExtractorEntry::new(&["hydra"], extract)
}
```

- [ ] **Step 9: Create core/fmt/mod.rs and core/fmt/table.rs**

`src/core/fmt/mod.rs`:
```rust
mod table;

pub struct FormatterEntry {
    pub name: &'static str,
    pub format: fn(&[String], &[Vec<serde_json::Value>]) -> String,
}

impl FormatterEntry {
    pub const fn new(name: &'static str, format: fn(&[String], &[Vec<serde_json::Value>]) -> String) -> Self {
        Self { name, format }
    }
}

inventory::collect!(FormatterEntry);

pub fn format(name: &str, columns: &[String], rows: &[Vec<serde_json::Value>]) -> String {
    for entry in inventory::iter::<FormatterEntry> {
        if entry.name == name {
            return (entry.format)(columns, rows);
        }
    }
    // Default: table
    for entry in inventory::iter::<FormatterEntry> {
        if entry.name == "table" {
            return (entry.format)(columns, rows);
        }
    }
    String::from("(no formatter found)")
}
```

`src/core/fmt/table.rs`:
```rust
use super::FormatterEntry;

fn format_table(columns: &[String], rows: &[Vec<serde_json::Value>]) -> String {
    if columns.is_empty() {
        return String::from("(0 rows)\n");
    }

    // Convert Values to display strings
    let str_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|v| match v {
                    serde_json::Value::Null => "NULL".to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    other => other.to_string(),
                })
                .collect()
        })
        .collect();

    // Calculate column widths
    let mut widths: Vec<usize> = columns.iter().map(|c| c.len()).collect();
    for row in &str_rows {
        for (i, val) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(val.len());
            }
        }
    }

    let mut out = String::new();

    // Header
    let header: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{:<width$}", c, width = widths[i]))
        .collect();
    out.push_str(&header.join(" | "));
    out.push('\n');

    // Separator
    let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    out.push_str(&sep.join("-+-"));
    out.push('\n');

    // Rows
    for row in &str_rows {
        let formatted: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let w = if i < widths.len() { widths[i] } else { v.len() };
                format!("{:<width$}", v, width = w)
            })
            .collect();
        out.push_str(&formatted.join(" | "));
        out.push('\n');
    }

    out.push_str(&format!("({} rows)\n", rows.len()));
    out
}

inventory::submit! {
    FormatterEntry::new("table", format_table)
}
```

- [ ] **Step 10: Create core/agent/mod.rs and core/agent/providers/mod.rs**

Copy the existing `src/agent/mod.rs` content into `src/core/agent/mod.rs`, but remove the `assistant`, `strategist`, and `tools` submodules. Keep only `providers` and the `AnyModel` enum + `create_model()` + `Agent` struct.

**IMPORTANT:** Adapt `create_model()` to the new flat `Config` struct. The existing code accesses `config.general.llm_provider` — change to `config.llm_provider`:

```rust
pub fn create_model(config: &crate::config::Config) -> Result<AnyModel, crate::error::Error> {
    match config.llm_provider.as_str() {
        "anthropic" => {
            let api_key = std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| crate::error::Error::Config("ANTHROPIC_API_KEY not set".into()))?;
            let model = Anthropic::<DynamicModel>::builder()
                .model_name(&config.llm_model)
                .api_key(api_key)
                .build()
                .map_err(|e| crate::error::Error::Config(format!("anthropic provider: {e}")))?;
            Ok(AnyModel::Anthropic(model))
        }
        "claude-code" => Ok(AnyModel::ClaudeCode(providers::ClaudeCodeProvider::new())),
        other => Err(crate::error::Error::Config(format!("unsupported llm_provider: {other}"))),
    }
}
```

Copy the existing `src/agent/providers/mod.rs` into `src/core/agent/providers/mod.rs` unchanged.

Create empty `src/core/agent/tools.rs` — will be implemented in Task 8. Define `ToolContext` here:

```rust
// src/core/agent/tools.rs
use std::sync::{Arc, Mutex};

pub struct ToolContext {
    pub conn: Arc<Mutex<rusqlite::Connection>>,
    pub session_id: String,
    pub event_id: i64,
}
```

The `core/agent/mod.rs` should declare:
```rust
pub mod providers;
pub mod tools;
// ... AnyModel, create_model(), Agent struct
// ToolContext is in tools.rs, re-export: pub use tools::ToolContext;
```

Remove references to old db schema (the test helpers that use `hosts` table, etc.) and all `#[cfg(test)] mod tests` blocks — those tests will be rewritten.

- [ ] **Step 11: Create core/mod.rs**

```rust
// src/core/mod.rs
pub mod db;
pub mod net;
pub mod extractors;
pub mod fmt;
pub mod agent;
```

- [ ] **Step 12: Create context.rs**

```rust
// src/context.rs
use crate::config::Config;
use rusqlite::Connection;

pub struct AppContext {
    pub conn: Connection,
    pub config: Config,
    pub session_id: String,
}
```

- [ ] **Step 13: Create cmd stubs**

`src/cmd/mod.rs`:
```rust
pub mod proxy;
pub mod sql;
pub mod extract;
```

`src/cmd/proxy/mod.rs`:
```rust
mod pty;

use crate::context::AppContext;
use crate::error::Error;

pub struct ProxyArgs {
    pub command: Vec<String>,
}

pub fn run(_ctx: &AppContext, _args: &ProxyArgs) -> Result<(), Error> {
    todo!()
}
```

`src/cmd/proxy/pty.rs`:
```rust
// PTY spawn and capture — will be implemented in Task 3
```

`src/cmd/sql/mod.rs`:
```rust
use crate::context::AppContext;
use crate::error::Error;

pub struct SqlArgs {
    pub query: String,
    pub json: bool,
}

pub fn run(_ctx: &AppContext, _args: &SqlArgs) -> Result<(), Error> {
    todo!()
}
```

`src/cmd/extract/mod.rs`:
```rust
mod extraction;

use crate::context::AppContext;
use crate::error::Error;

pub struct ExtractArgs {
    pub event_id: i64,
    pub force: bool,
}

pub fn run(_ctx: &AppContext, _args: &ExtractArgs) -> Result<(), Error> {
    todo!()
}
```

`src/cmd/extract/extraction.rs`:
```rust
// LLM extraction agent setup — will be implemented in Task 8
```

- [ ] **Step 14: Create cli.rs with Clap dispatch**

```rust
// src/cli.rs
use clap::{Parser, Subcommand};
use crate::cmd;
use redtrail::config::Config;
use redtrail::context::AppContext;
use redtrail::core;
use redtrail::error::Error;

#[derive(Parser)]
#[command(name = "rt", about = "Terminal activity capture and knowledge extraction")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a command through a PTY, capturing output and extracting facts
    Proxy {
        /// The command and arguments to run
        #[arg(trailing_var_arg = true, required = true)]
        command: Vec<String>,
    },
    /// Execute raw SQL against the database
    Sql {
        /// SQL query to execute
        query: String,
        /// Output as JSON instead of ASCII table
        #[arg(long)]
        json: bool,
    },
    /// Run LLM-based extraction on a stored event
    Extract {
        /// Event ID to extract from
        event_id: i64,
        /// Force re-extraction even if already extracted
        #[arg(long)]
        force: bool,
    },
}

pub fn run() -> Result<(), Error> {
    let cli = Cli::parse();

    let db_path = core::db::global_db_path()?;
    let conn = core::db::open(db_path.to_str().unwrap())?;
    let cwd = std::env::current_dir()?;
    let workspace_path = cwd.to_string_lossy().to_string();
    let session_id = core::db::ensure_session(&conn, &workspace_path)?;
    let config = Config::default();

    let ctx = AppContext { conn, config, session_id };

    match cli.command {
        Commands::Proxy { command } => {
            let args = cmd::proxy::ProxyArgs { command };
            cmd::proxy::run(&ctx, &args)
        }
        Commands::Sql { query, json } => {
            let args = cmd::sql::SqlArgs { query, json };
            cmd::sql::run(&ctx, &args)
        }
        Commands::Extract { event_id, force } => {
            let args = cmd::extract::ExtractArgs { event_id, force };
            cmd::extract::run(&ctx, &args)
        }
    }
}
```

- [ ] **Step 15: Create lib.rs and main.rs**

The crate needs both a library target (for integration tests to import) and a binary target.

`src/lib.rs`:
```rust
// src/lib.rs — library root, integration tests import from here
pub mod core;
pub mod config;
pub mod context;
pub mod error;
```

`src/main.rs`:
```rust
// src/main.rs
mod cli;
mod cmd;

fn main() {
    if let Err(e) = cli::run() {
        eprintln!("[rt] error: {e}");
        std::process::exit(1);
    }
}
```

**IMPORTANT:** `cli.rs` and all `cmd/` modules must use `redtrail::` paths for shared modules (core, config, context, error) since those live in the library crate. Only `cli` and `cmd` are private to the binary crate.

Update `cli.rs` imports (from Step 14):
- `use crate::cmd;` (binary-private)
- `use redtrail::config::Config;` (library)
- `use redtrail::context::AppContext;` (library)
- `use redtrail::core;` (library)
- `use redtrail::error::Error;` (library)

Similarly, all `cmd/*/mod.rs` stubs (Step 13) should use:
```rust
use redtrail::context::AppContext;
use redtrail::error::Error;
```
instead of `use crate::context::AppContext;`.

- [ ] **Step 16: Verify compilation**

Run: `cargo build 2>&1`
Expected: Compiles with warnings about unused/todo items but no errors.

- [ ] **Step 17: Commit**

```bash
git add -A
git commit -m "refactor: gut codebase and create skeleton for events/facts/relations redesign"
```

---

## Task 2: Implement and test core::db

**Files:**
- Modify: `src/core/db.rs` (already created — verify it compiles with tests)
- Create: `tests/db_test.rs`

- [ ] **Step 1: Write db integration tests**

Create `tests/db_test.rs`:

```rust
use redtrail::core::db;
use redtrail::core::extractors::{Fact, Relation};

#[test]
fn open_in_memory_creates_schema() {
    let conn = db::open_in_memory().unwrap();
    // Verify tables exist by querying them
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn ensure_session_creates_new_session() {
    let conn = db::open_in_memory().unwrap();
    let id = db::ensure_session(&conn, "/tmp/test-project").unwrap();
    assert!(id.starts_with("test-project-"));

    // Verify it's in the DB
    let name: String = conn
        .query_row("SELECT name FROM sessions WHERE id = ?1", [&id], |r| r.get(0))
        .unwrap();
    assert_eq!(name, "test-project");
}

#[test]
fn ensure_session_reuses_existing() {
    let conn = db::open_in_memory().unwrap();
    let id1 = db::ensure_session(&conn, "/tmp/test-project").unwrap();
    let id2 = db::ensure_session(&conn, "/tmp/test-project").unwrap();
    assert_eq!(id1, id2);
}

#[test]
fn insert_event_and_query_back() {
    let conn = db::open_in_memory().unwrap();
    let sid = db::ensure_session(&conn, "/tmp/test").unwrap();

    let eid = db::insert_event(
        &conn, &sid, "nmap -sV 10.10.10.1", Some("nmap"),
        0, 1500, "22/tcp open ssh", "abc123",
    ).unwrap();

    assert!(eid > 0);

    let (cmd, tool, status): (String, Option<String>, String) = conn
        .query_row(
            "SELECT command, tool, extraction_status FROM events WHERE id = ?1",
            [eid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
    assert_eq!(cmd, "nmap -sV 10.10.10.1");
    assert_eq!(tool.as_deref(), Some("nmap"));
    assert_eq!(status, "stored");
}

#[test]
fn insert_fact_and_query_back() {
    let conn = db::open_in_memory().unwrap();
    let sid = db::ensure_session(&conn, "/tmp/test").unwrap();
    let eid = db::insert_event(&conn, &sid, "nmap", None, 0, 0, "", "").unwrap();

    let attrs = serde_json::json!({"ip": "10.10.10.1", "status": "up"});
    db::insert_fact(&conn, &sid, eid, "host", "host:10.10.10.1", &attrs, 1.0, "regex").unwrap();

    let (ft, key, attr_str): (String, String, String) = conn
        .query_row(
            "SELECT fact_type, key, attributes FROM facts WHERE session_id = ?1",
            [&sid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
    assert_eq!(ft, "host");
    assert_eq!(key, "host:10.10.10.1");
    let parsed: serde_json::Value = serde_json::from_str(&attr_str).unwrap();
    assert_eq!(parsed["ip"], "10.10.10.1");
}

#[test]
fn fact_upsert_merges_attributes() {
    let conn = db::open_in_memory().unwrap();
    let sid = db::ensure_session(&conn, "/tmp/test").unwrap();
    let eid = db::insert_event(&conn, &sid, "nmap", None, 0, 0, "", "").unwrap();

    // First insert
    let attrs1 = serde_json::json!({"ip": "10.10.10.1", "status": "up"});
    db::insert_fact(&conn, &sid, eid, "host", "host:10.10.10.1", &attrs1, 1.0, "regex").unwrap();

    // Upsert with new attribute
    let attrs2 = serde_json::json!({"hostname": "target.htb"});
    db::insert_fact(&conn, &sid, eid, "host", "host:10.10.10.1", &attrs2, 0.8, "llm").unwrap();

    let attr_str: String = conn
        .query_row(
            "SELECT attributes FROM facts WHERE key = 'host:10.10.10.1'",
            [], |r| r.get(0),
        ).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&attr_str).unwrap();
    // Both attributes should be present (shallow merge via json_patch)
    assert_eq!(parsed["ip"], "10.10.10.1");
    assert_eq!(parsed["hostname"], "target.htb");

    // Confidence should be MAX(1.0, 0.8) = 1.0
    let conf: f64 = conn
        .query_row(
            "SELECT confidence FROM facts WHERE key = 'host:10.10.10.1'",
            [], |r| r.get(0),
        ).unwrap();
    assert!((conf - 1.0).abs() < 0.001);

    // Source should stay "regex" since regex confidence (1.0) > llm confidence (0.8)
    let source: String = conn
        .query_row(
            "SELECT source FROM facts WHERE key = 'host:10.10.10.1'",
            [], |r| r.get(0),
        ).unwrap();
    assert_eq!(source, "regex");
}

#[test]
fn insert_relation_and_dedup() {
    let conn = db::open_in_memory().unwrap();
    let sid = db::ensure_session(&conn, "/tmp/test").unwrap();

    db::insert_relation(&conn, &sid, "service:10.10.10.1:22/tcp", "host:10.10.10.1", "runs_on").unwrap();
    // Duplicate — should be ignored
    db::insert_relation(&conn, &sid, "service:10.10.10.1:22/tcp", "host:10.10.10.1", "runs_on").unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM relations WHERE session_id = ?1", [&sid], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn store_extraction_transaction() {
    let conn = db::open_in_memory().unwrap();
    let sid = db::ensure_session(&conn, "/tmp/test").unwrap();
    let eid = db::insert_event(&conn, &sid, "nmap -sV 10.10.10.1", Some("nmap"), 0, 1000, "output", "hash").unwrap();

    let facts = vec![
        Fact { fact_type: "host".into(), key: "host:10.10.10.1".into(), attributes: serde_json::json!({"ip": "10.10.10.1"}) },
        Fact { fact_type: "service".into(), key: "service:10.10.10.1:22/tcp".into(), attributes: serde_json::json!({"port": 22, "service": "ssh"}) },
    ];
    let relations = vec![
        Relation { from_key: "service:10.10.10.1:22/tcp".into(), to_key: "host:10.10.10.1".into(), relation_type: "runs_on".into() },
    ];

    db::store_extraction(&conn, &sid, eid, &facts, &relations).unwrap();

    let fact_count: i64 = conn.query_row("SELECT COUNT(*) FROM facts", [], |r| r.get(0)).unwrap();
    assert_eq!(fact_count, 2);

    let rel_count: i64 = conn.query_row("SELECT COUNT(*) FROM relations", [], |r| r.get(0)).unwrap();
    assert_eq!(rel_count, 1);

    let status: String = conn.query_row("SELECT extraction_status FROM events WHERE id = ?1", [eid], |r| r.get(0)).unwrap();
    assert_eq!(status, "extracted");
}
```

- [ ] **Step 2: Make sure db.rs functions are pub and exported from lib**

Ensure `src/core/mod.rs` has `pub mod db;` and that all functions in `db.rs` that tests use are `pub`. Also ensure `src/main.rs` re-exports core for integration tests:

Add to `src/main.rs`:
```rust
pub mod core;
pub mod config;
pub mod error;
// ... (plus the private mods)
```

Actually, since this is a binary crate, integration tests need a lib target. Add `src/lib.rs`:

```rust
// src/lib.rs
pub mod core;
pub mod config;
pub mod context;
pub mod error;
```

And update `main.rs` to use it:
```rust
// src/main.rs
mod cli;
mod cmd;

fn main() {
    if let Err(e) = cli::run() {
        eprintln!("[rt] error: {e}");
        std::process::exit(1);
    }
}
```

Update `cli.rs` imports to use `redtrail::` paths for shared modules and `crate::cmd` for command modules.

- [ ] **Step 3: Run tests**

Run: `cargo test --test db_test 2>&1`
Expected: All tests PASS.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: implement core::db with events/facts/relations schema and full test coverage"
```

---

## Task 3: Implement cmd::proxy (PTY capture + extraction)

**Files:**
- Modify: `src/cmd/proxy/mod.rs`
- Modify: `src/cmd/proxy/pty.rs`

- [ ] **Step 1: Implement pty.rs**

```rust
// src/cmd/proxy/pty.rs
use crate::error::Error;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::time::Instant;

pub struct PtyResult {
    pub exit_code: i32,
    pub duration_ms: i64,
    pub output: String,
}

pub fn spawn_and_capture(args: &[String]) -> Result<PtyResult, Error> {
    let cwd = std::env::current_dir()?;

    // Inherit terminal size
    let (rows, cols) = terminal_size().unwrap_or((24, 80));

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;

    let mut cmd = CommandBuilder::new(&args[0]);
    for arg in &args[1..] {
        cmd.arg(arg);
    }
    cmd.cwd(&cwd);

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;

    let start = Instant::now();
    let mut output = Vec::new();
    let mut buf = [0u8; 4096];

    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                std::io::stdout().write_all(&buf[..n])?;
                std::io::stdout().flush()?;
                output.extend_from_slice(&buf[..n]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Other => break,
            Err(e) => return Err(Error::Io(e)),
        }
    }

    let status = child
        .wait()
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
    let exit_code = status.exit_code() as i32;
    let duration_ms = start.elapsed().as_millis() as i64;
    let output_str = String::from_utf8_lossy(&output).to_string();

    Ok(PtyResult {
        exit_code,
        duration_ms,
        output: output_str,
    })
}

fn terminal_size() -> Option<(u16, u16)> {
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_row > 0 {
            Some((ws.ws_row, ws.ws_col))
        } else {
            None
        }
    }
}
```

Note: add `libc` dependency to `Cargo.toml` if `portable-pty` doesn't already expose it. Check first — if it doesn't work without libc, add `libc = "0.2"` to Cargo.toml.

- [ ] **Step 2: Implement proxy/mod.rs**

```rust
// src/cmd/proxy/mod.rs
mod pty;

use crate::context::AppContext;
use crate::error::Error;
use redtrail::core;
use sha2::{Digest, Sha256};

pub struct ProxyArgs {
    pub command: Vec<String>,
}

pub fn run(ctx: &AppContext, args: &ProxyArgs) -> Result<(), Error> {
    if args.command.is_empty() {
        return Ok(());
    }

    let cmd_str = args.command.join(" ");
    let tool = core::extractors::detect_tool(&cmd_str, None);

    // Spawn PTY and capture
    let result = pty::spawn_and_capture(&args.command)?;

    // Hash output for dedup
    let hash = format!("{:x}", Sha256::digest(result.output.as_bytes()));

    // Store event
    let event_id = core::db::insert_event(
        &ctx.conn,
        &ctx.session_id,
        &cmd_str,
        tool.as_deref(),
        result.exit_code,
        result.duration_ms,
        &result.output,
        &hash,
    )?;

    // Try deterministic extraction
    let extraction = core::extractors::synthesize(&cmd_str, tool.as_deref(), &result.output);

    if !extraction.is_empty() {
        core::db::store_extraction(
            &ctx.conn, &ctx.session_id, event_id,
            &extraction.facts, &extraction.relations,
        )?;

        // Print summary
        let mut summary_parts = Vec::new();
        let fact_types: std::collections::HashMap<&str, usize> = extraction.facts.iter()
            .fold(std::collections::HashMap::new(), |mut m, f| {
                *m.entry(f.fact_type.as_str()).or_insert(0) += 1;
                m
            });
        for (ft, count) in &fact_types {
            summary_parts.push(format!("{count} {ft}s"));
        }
        if !extraction.relations.is_empty() {
            summary_parts.push(format!("{} relations", extraction.relations.len()));
        }
        eprintln!("[rt] extracted {}", summary_parts.join(", "));
    }

    if result.exit_code != 0 {
        std::process::exit(result.exit_code);
    }
    Ok(())
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build 2>&1`
Expected: Compiles. Fix any import issues between binary and library crate.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: implement proxy command with PTY capture and sync extraction"
```

---

## Task 4: Implement cmd::sql

**Files:**
- Modify: `src/cmd/sql/mod.rs`
- Create: `tests/fmt_test.rs`

- [ ] **Step 1: Write fmt integration tests**

Create `tests/fmt_test.rs`:

```rust
use redtrail::core::fmt;

#[test]
fn format_table_basic() {
    let cols = vec!["name".into(), "value".into()];
    let rows = vec![
        vec![serde_json::json!("host"), serde_json::json!("10.10.10.1")],
        vec![serde_json::json!("port"), serde_json::json!(22)],
    ];
    let out = fmt::format("table", &cols, &rows);
    assert!(out.contains("name"));
    assert!(out.contains("value"));
    assert!(out.contains("10.10.10.1"));
    assert!(out.contains("22"));
    assert!(out.contains("(2 rows)"));
}

#[test]
fn format_table_empty() {
    let cols: Vec<String> = vec![];
    let rows: Vec<Vec<serde_json::Value>> = vec![];
    let out = fmt::format("table", &cols, &rows);
    assert!(out.contains("(0 rows)"));
}

#[test]
fn format_table_null_values() {
    let cols = vec!["col".into()];
    let rows = vec![vec![serde_json::Value::Null]];
    let out = fmt::format("table", &cols, &rows);
    assert!(out.contains("NULL"));
}

#[test]
fn format_table_wide_values() {
    let cols = vec!["short".into(), "long_column_name".into()];
    let rows = vec![
        vec![serde_json::json!("a"), serde_json::json!("b")],
    ];
    let out = fmt::format("table", &cols, &rows);
    // Column header should pad to at least its own length
    assert!(out.contains("long_column_name"));
}
```

- [ ] **Step 2: Run fmt tests**

Run: `cargo test --test fmt_test 2>&1`
Expected: All PASS.

- [ ] **Step 3: Implement cmd::sql**

```rust
// src/cmd/sql/mod.rs
use crate::context::AppContext;
use crate::error::Error;
use redtrail::core::fmt;

pub struct SqlArgs {
    pub query: String,
    pub json: bool,
}

pub fn run(ctx: &AppContext, args: &SqlArgs) -> Result<(), Error> {
    let sql = args.query.trim();

    if is_read_query(sql) {
        let mut stmt = ctx.conn.prepare(sql).map_err(|e| Error::Db(e.to_string()))?;
        let col_count = stmt.column_count();
        let columns: Vec<String> = (0..col_count)
            .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
            .collect();

        let rows: Vec<Vec<serde_json::Value>> = stmt
            .query_map([], |row| {
                let mut vals = Vec::new();
                for i in 0..col_count {
                    let val = row
                        .get::<_, rusqlite::types::Value>(i)
                        .map(|v| match v {
                            rusqlite::types::Value::Null => serde_json::Value::Null,
                            rusqlite::types::Value::Integer(n) => serde_json::json!(n),
                            rusqlite::types::Value::Real(f) => serde_json::json!(f),
                            rusqlite::types::Value::Text(s) => serde_json::json!(s),
                            rusqlite::types::Value::Blob(b) => serde_json::json!(format!("<blob {} bytes>", b.len())),
                        })
                        .unwrap_or(serde_json::Value::Null);
                    vals.push(val);
                }
                Ok(vals)
            })
            .map_err(|e| Error::Db(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        if args.json {
            let json_rows: Vec<serde_json::Value> = rows.iter().map(|row| {
                let mut map = serde_json::Map::new();
                for (i, val) in row.iter().enumerate() {
                    map.insert(columns[i].clone(), val.clone());
                }
                serde_json::Value::Object(map)
            }).collect();
            println!("{}", serde_json::to_string_pretty(&json_rows).unwrap());
        } else {
            print!("{}", fmt::format("table", &columns, &rows));
        }
    } else {
        let affected = ctx.conn.execute(sql, []).map_err(|e| Error::Db(e.to_string()))?;
        if args.json {
            println!("{}", serde_json::json!({"affected_rows": affected}));
        } else {
            println!("{affected} rows affected");
        }
    }

    Ok(())
}

fn is_read_query(sql: &str) -> bool {
    let upper = sql.trim_start().to_uppercase();
    upper.starts_with("SELECT")
        || upper.starts_with("PRAGMA")
        || upper.starts_with("EXPLAIN")
        || upper.starts_with("WITH")
}
```

- [ ] **Step 4: Verify compilation and run all tests**

Run: `cargo test 2>&1`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: implement sql command with ASCII table and JSON output"
```

---

## Task 5: Implement nmap extractor

**Files:**
- Modify: `src/core/extractors/nmap.rs`
- Create: `tests/synthesize_test.rs`
- Fixture: `eval/tests/fixtures/nmap-scan.txt` (already exists)

- [ ] **Step 1: Write synthesize tests for nmap**

Create `tests/synthesize_test.rs`:

```rust
use redtrail::core::extractors;

fn load_fixture(name: &str) -> String {
    std::fs::read_to_string(format!("eval/tests/fixtures/{name}")).unwrap()
}

// --- detect_tool tests ---

#[test]
fn detect_tool_simple() {
    assert_eq!(extractors::detect_tool("nmap -sV 10.10.10.1", None).as_deref(), Some("nmap"));
}

#[test]
fn detect_tool_with_hint() {
    assert_eq!(extractors::detect_tool("foo bar", Some("nmap")).as_deref(), Some("nmap"));
}

#[test]
fn detect_tool_skips_sudo() {
    assert_eq!(extractors::detect_tool("sudo nmap -sV 10.10.10.1", None).as_deref(), Some("nmap"));
}

#[test]
fn detect_tool_skips_proxychains() {
    assert_eq!(extractors::detect_tool("proxychains nmap -sV 10.10.10.1", None).as_deref(), Some("nmap"));
}

#[test]
fn detect_tool_skips_env_vars() {
    assert_eq!(extractors::detect_tool("MY_VAR=foo sudo nmap -sV 10.10.10.1", None).as_deref(), Some("nmap"));
}

#[test]
fn detect_tool_skips_multiple_env_vars() {
    assert_eq!(extractors::detect_tool("A=1 B=2 gobuster dir -u http://target", None).as_deref(), Some("gobuster"));
}

#[test]
fn detect_tool_empty_command() {
    assert_eq!(extractors::detect_tool("", None), None);
}

// --- nmap extractor tests ---

#[test]
fn synthesize_nmap_basic() {
    let output = load_fixture("nmap-scan.txt");
    let result = extractors::synthesize("nmap -sV -sC -p- 10.10.10.42", Some("nmap"), &output);

    // Should extract the host
    let hosts: Vec<_> = result.facts.iter().filter(|f| f.fact_type == "host").collect();
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].key, "host:10.10.10.42");
    assert_eq!(hosts[0].attributes["ip"], "10.10.10.42");

    // Should extract services
    let services: Vec<_> = result.facts.iter().filter(|f| f.fact_type == "service").collect();
    assert!(services.len() >= 4); // 22, 80, 443, 3306

    // Check SSH service
    let ssh = services.iter().find(|s| s.attributes["port"] == 22).unwrap();
    assert_eq!(ssh.key, "service:10.10.10.42:22/tcp");
    assert_eq!(ssh.attributes["service"], "ssh");
    assert!(ssh.attributes["version"].as_str().unwrap().contains("OpenSSH"));

    // Check HTTP service
    let http = services.iter().find(|s| s.attributes["port"] == 80).unwrap();
    assert_eq!(http.attributes["service"], "http");

    // Check MySQL service
    let mysql = services.iter().find(|s| s.attributes["port"] == 3306).unwrap();
    assert_eq!(mysql.attributes["service"], "mysql");

    // Should have runs_on relations
    let runs_on: Vec<_> = result.relations.iter().filter(|r| r.relation_type == "runs_on").collect();
    assert!(runs_on.len() >= 4);
    assert!(runs_on.iter().all(|r| r.to_key == "host:10.10.10.42"));
}

#[test]
fn synthesize_nmap_with_hostname() {
    let output = "Nmap scan report for board.htb (10.10.10.42)\n22/tcp open ssh OpenSSH 8.9\n";
    let result = extractors::synthesize("nmap 10.10.10.42", Some("nmap"), output);

    let host = result.facts.iter().find(|f| f.fact_type == "host").unwrap();
    assert_eq!(host.attributes["hostname"], "board.htb");
    assert_eq!(host.attributes["ip"], "10.10.10.42");
}

#[test]
fn synthesize_nmap_os_detection() {
    let output = "Nmap scan report for 10.10.10.42\n22/tcp open ssh\nOS details: Linux 5.4\n";
    let result = extractors::synthesize("nmap -O 10.10.10.42", Some("nmap"), output);

    let os = result.facts.iter().find(|f| f.fact_type == "os_info").unwrap();
    assert_eq!(os.attributes["os"], "Linux 5.4");
}

#[test]
fn synthesize_nmap_empty_output() {
    let result = extractors::synthesize("nmap 10.10.10.1", Some("nmap"), "");
    assert!(result.is_empty());
}

#[test]
fn synthesize_unknown_tool() {
    let result = extractors::synthesize("curl http://example.com", Some("curl"), "hello world");
    assert!(result.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test synthesize_test 2>&1`
Expected: Tests that check for extracted facts FAIL (extractors are stubs returning empty).

- [ ] **Step 3: Implement nmap extractor**

Replace `src/core/extractors/nmap.rs`:

```rust
use super::{ExtractionResult, ExtractorEntry, Fact, Relation};
use regex::Regex;

fn extract(_command: &str, output: &str) -> ExtractionResult {
    let mut facts = Vec::new();
    let mut relations = Vec::new();
    let mut current_host: Option<String> = None;

    let re_host = Regex::new(r"Nmap scan report for (?:(\S+) \()?(\d+\.\d+\.\d+\.\d+)\)?").unwrap();
    let re_port = Regex::new(r"(\d+)/(tcp|udp)\s+(open|closed|filtered)\s+(\S+)(?:\s+(.+))?").unwrap();
    let re_os = Regex::new(r"OS details?:\s*(.+)").unwrap();

    for line in output.lines() {
        if let Some(caps) = re_host.captures(line) {
            let ip = caps.get(2).unwrap().as_str().to_string();
            let hostname = caps.get(1).map(|m| m.as_str().to_string());

            let mut attrs = serde_json::json!({"ip": ip, "status": "up"});
            if let Some(ref h) = hostname {
                attrs["hostname"] = serde_json::json!(h);
            }

            facts.push(Fact {
                fact_type: "host".into(),
                key: format!("host:{ip}"),
                attributes: attrs,
            });
            current_host = Some(ip);
        }

        if let (Some(ref host), Some(caps)) = (&current_host, re_port.captures(line)) {
            let port: u16 = match caps[1].parse() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let proto = &caps[2];
            let state = &caps[3];
            let service = &caps[4];
            let version = caps.get(5).map(|m| m.as_str().trim().to_string());

            if state != "open" {
                continue;
            }

            let mut attrs = serde_json::json!({
                "ip": host,
                "port": port,
                "protocol": proto,
                "service": service,
            });
            if let Some(ref v) = version {
                if !v.is_empty() {
                    attrs["version"] = serde_json::json!(v);
                }
            }

            let key = format!("service:{host}:{port}/{proto}");
            facts.push(Fact {
                fact_type: "service".into(),
                key: key.clone(),
                attributes: attrs,
            });
            relations.push(Relation {
                from_key: key,
                to_key: format!("host:{host}"),
                relation_type: "runs_on".into(),
            });
        }

        if let Some(caps) = re_os.captures(line) {
            if let Some(ref host) = current_host {
                let os = caps[1].trim().to_string();
                let key = format!("os:{host}");
                facts.push(Fact {
                    fact_type: "os_info".into(),
                    key: key.clone(),
                    attributes: serde_json::json!({"ip": host, "os": os}),
                });
                relations.push(Relation {
                    from_key: key,
                    to_key: format!("host:{host}"),
                    relation_type: "describes".into(),
                });
            }
        }
    }

    ExtractionResult { facts, relations }
}

inventory::submit! {
    ExtractorEntry::new(&["nmap"], extract)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test synthesize_test 2>&1`
Expected: All nmap tests PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: implement nmap extractor with host, service, and OS parsing"
```

---

## Task 6: Implement web_enum and hydra extractors

**Files:**
- Modify: `src/core/extractors/web_enum.rs`
- Modify: `src/core/extractors/hydra.rs`
- Modify: `tests/synthesize_test.rs`
- Create: `eval/tests/fixtures/hydra-basic.txt`

- [ ] **Step 1: Add web_enum and hydra tests to synthesize_test.rs**

Append to `tests/synthesize_test.rs`:

```rust
// --- web_enum extractor tests ---

#[test]
fn synthesize_gobuster_basic() {
    let output = load_fixture("gobuster-scan.txt");
    let result = extractors::synthesize(
        "gobuster dir -u http://10.10.10.42 -w /usr/share/wordlists/dirb/common.txt",
        Some("gobuster"),
        &output,
    );

    let paths: Vec<_> = result.facts.iter().filter(|f| f.fact_type == "web_path").collect();
    assert!(paths.len() >= 9); // /admin, /api, /backup, /cgi-bin, /config, /dashboard, /images, /index.html, /robots.txt

    // Check /admin path
    let admin = paths.iter().find(|p| p.attributes["path"] == "/admin").unwrap();
    assert_eq!(admin.attributes["status_code"], 301);

    // Check /api path
    let api = paths.iter().find(|p| p.attributes["path"] == "/api").unwrap();
    assert_eq!(api.attributes["status_code"], 200);
    assert_eq!(api.attributes["content_length"], 1245);

    // Should have served_by relations
    assert!(!result.relations.is_empty());
}

#[test]
fn synthesize_gobuster_extracts_target_from_command() {
    let output = "/test (Status: 200) [Size: 100]\n";
    let result = extractors::synthesize(
        "gobuster dir -u http://10.10.10.42:8080 -w wordlist.txt",
        Some("gobuster"),
        output,
    );

    let path = &result.facts[0];
    assert_eq!(path.attributes["ip"], "10.10.10.42");
    assert_eq!(path.attributes["port"], 8080);
}

#[test]
fn synthesize_ffuf_format() {
    let output = "admin                   [Status: 200, Size: 1234, Words: 56, Lines: 12]\n";
    let result = extractors::synthesize(
        "ffuf -u http://10.10.10.1/FUZZ -w wordlist.txt",
        Some("ffuf"),
        output,
    );

    assert!(!result.facts.is_empty());
    let path = &result.facts[0];
    assert_eq!(path.fact_type, "web_path");
    assert_eq!(path.attributes["status_code"], 200);
}

#[test]
fn synthesize_gobuster_empty_output() {
    let result = extractors::synthesize("gobuster dir -u http://10.10.10.1 -w w.txt", Some("gobuster"), "");
    assert!(result.is_empty());
}

// --- hydra extractor tests ---

#[test]
fn synthesize_hydra_basic() {
    let output = "[22][ssh] host: 10.10.10.1   login: admin   password: secret123\n\
                  [22][ssh] host: 10.10.10.1   login: root   password: toor\n";
    let result = extractors::synthesize("hydra -l admin -P passwords.txt ssh://10.10.10.1", Some("hydra"), output);

    let creds: Vec<_> = result.facts.iter().filter(|f| f.fact_type == "credential").collect();
    assert_eq!(creds.len(), 2);

    let admin_cred = creds.iter().find(|c| c.attributes["username"] == "admin").unwrap();
    assert_eq!(admin_cred.attributes["password"], "secret123");
    assert_eq!(admin_cred.attributes["service"], "ssh");
    assert_eq!(admin_cred.attributes["ip"], "10.10.10.1");

    // Should have authenticates_to relations
    let auth_rels: Vec<_> = result.relations.iter()
        .filter(|r| r.relation_type == "authenticates_to")
        .collect();
    assert_eq!(auth_rels.len(), 2);
}

#[test]
fn synthesize_hydra_no_results() {
    let output = "Hydra (https://github.com/vanhauser-thc/thc-hydra)\n\
                  [DATA] attacking ssh://10.10.10.1:22/\n\
                  0 valid password found\n";
    let result = extractors::synthesize("hydra -l admin -P pass.txt ssh://10.10.10.1", Some("hydra"), output);
    assert!(result.facts.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test synthesize_test 2>&1`
Expected: New web_enum and hydra tests FAIL.

- [ ] **Step 3: Implement web_enum extractor**

Replace `src/core/extractors/web_enum.rs`:

```rust
use super::{ExtractionResult, ExtractorEntry, Fact, Relation};
use regex::Regex;

fn parse_target_from_command(command: &str) -> (String, u16) {
    // Extract -u URL from command
    let re = Regex::new(r"-u\s+https?://([^/:]+)(?::(\d+))?").unwrap();
    if let Some(caps) = re.captures(command) {
        let ip = caps[1].to_string();
        let port = caps.get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(if command.contains("https://") { 443 } else { 80 });
        return (ip, port);
    }
    // Fallback: try FUZZ URL pattern (ffuf style)
    let re_fuzz = Regex::new(r"https?://([^/:]+)(?::(\d+))?").unwrap();
    if let Some(caps) = re_fuzz.captures(command) {
        let ip = caps[1].to_string();
        let port = caps.get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(80);
        return (ip, port);
    }
    ("unknown".into(), 80)
}

fn extract(command: &str, output: &str) -> ExtractionResult {
    let mut facts = Vec::new();
    let mut relations = Vec::new();

    let (target_ip, target_port) = parse_target_from_command(command);

    // Gobuster: /path (Status: 200) [Size: 1234] [--> redirect]
    let re_gobuster = Regex::new(
        r"(/\S+)\s+\(Status:\s*(\d+)\)\s*\[Size:\s*(\d+)\](?:\s*\[--> ([^\]]+)\])?"
    ).unwrap();

    // ffuf: path [Status: 200, Size: 1234, Words: N, Lines: N]
    let re_ffuf = Regex::new(
        r"^(\S+)\s+\[Status:\s*(\d+),\s*Size:\s*(\d+)"
    ).unwrap();

    for line in output.lines() {
        let (path, status, size, redirect) = if let Some(caps) = re_gobuster.captures(line) {
            (
                caps[1].to_string(),
                caps[2].parse::<u16>().unwrap_or(0),
                caps[3].parse::<u64>().unwrap_or(0),
                caps.get(4).map(|m| m.as_str().to_string()),
            )
        } else if let Some(caps) = re_ffuf.captures(line) {
            (
                format!("/{}", &caps[1]),
                caps[2].parse::<u16>().unwrap_or(0),
                caps[3].parse::<u64>().unwrap_or(0),
                None,
            )
        } else {
            continue;
        };

        let key = format!("web_path:{target_ip}:{target_port}:{path}");
        let mut attrs = serde_json::json!({
            "ip": target_ip,
            "port": target_port,
            "path": path,
            "status_code": status,
            "content_length": size,
        });
        if let Some(ref redir) = redirect {
            attrs["redirect_to"] = serde_json::json!(redir);
        }

        facts.push(Fact {
            fact_type: "web_path".into(),
            key: key.clone(),
            attributes: attrs,
        });

        relations.push(Relation {
            from_key: key,
            to_key: format!("service:{target_ip}:{target_port}/tcp"),
            relation_type: "served_by".into(),
        });
    }

    ExtractionResult { facts, relations }
}

inventory::submit! {
    ExtractorEntry::new(&["gobuster", "ffuf", "feroxbuster", "dirb", "wfuzz"], extract)
}
```

- [ ] **Step 4: Implement hydra extractor**

Replace `src/core/extractors/hydra.rs`:

```rust
use super::{ExtractionResult, ExtractorEntry, Fact, Relation};
use regex::Regex;

fn extract(_command: &str, output: &str) -> ExtractionResult {
    let mut facts = Vec::new();
    let mut relations = Vec::new();

    // [22][ssh] host: 10.10.10.1   login: admin   password: secret
    let re = Regex::new(
        r"\[(\d+)\]\[(\w+)\]\s+host:\s*(\S+)\s+login:\s*(\S+)\s+password:\s*(.+)"
    ).unwrap();

    for line in output.lines() {
        if let Some(caps) = re.captures(line) {
            let port: u16 = caps[1].parse().unwrap_or(0);
            let service = caps[2].to_string();
            let ip = caps[3].to_string();
            let username = caps[4].to_string();
            let password = caps[5].trim().to_string();

            let key = format!("credential:{username}:{service}:{ip}");
            facts.push(Fact {
                fact_type: "credential".into(),
                key: key.clone(),
                attributes: serde_json::json!({
                    "ip": ip,
                    "port": port,
                    "service": service,
                    "username": username,
                    "password": password,
                }),
            });

            relations.push(Relation {
                from_key: key,
                to_key: format!("service:{ip}:{port}/tcp"),
                relation_type: "authenticates_to".into(),
            });
        }
    }

    ExtractionResult { facts, relations }
}

inventory::submit! {
    ExtractorEntry::new(&["hydra"], extract)
}
```

- [ ] **Step 5: Run all tests**

Run: `cargo test 2>&1`
Expected: All tests PASS.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: implement web_enum and hydra extractors"
```

---

## Task 7: Write live e2e tests

**Files:**
- Rewrite: `eval/tests/*.sh` (delete old tests, create new ones)
- Modify: `eval/score.sh` if needed

- [ ] **Step 1: Delete old eval tests and goals**

```bash
rm -f eval/tests/feature-*.sh
rm -rf eval/goals/
rm -f eval/.last_passing
rm -f eval/results.tsv
```

- [ ] **Step 2: Create proxy-captures-event.sh**

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
if [[ -n "${RT_BIN:-}" ]]; then
    RT="$RT_BIN"
else
    cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null
    RT="$REPO_ROOT/target/release/rt"
fi

ORIG_HOME="$HOME"
TMPDIR=$(mktemp -d)
trap 'export HOME="$ORIG_HOME"; rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"

cd "$TMPDIR"

# Run a simple command through proxy
"$RT" proxy echo "hello redtrail" > /dev/null 2>&1 || true

# Verify event was stored
RESULT=$("$RT" sql "SELECT command, extraction_status FROM events LIMIT 1" --json 2>/dev/null)
echo "$RESULT" | grep -q "echo" || { echo "FAIL: event not stored"; exit 1; }
echo "$RESULT" | grep -q "stored" || { echo "FAIL: extraction_status should be stored"; exit 1; }

echo "PASS"
```

- [ ] **Step 3: Create proxy-auto-session.sh**

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
if [[ -n "${RT_BIN:-}" ]]; then RT="$RT_BIN"; else
    cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null
    RT="$REPO_ROOT/target/release/rt"
fi

ORIG_HOME="$HOME"
TMPDIR=$(mktemp -d)
trap 'export HOME="$ORIG_HOME"; rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"

mkdir -p "$TMPDIR/my-project"
cd "$TMPDIR/my-project"

# First proxy command should auto-create session
"$RT" proxy echo test > /dev/null 2>&1 || true

RESULT=$("$RT" sql "SELECT id, name FROM sessions LIMIT 1" --json 2>/dev/null)
echo "$RESULT" | grep -q "my-project" || { echo "FAIL: session not auto-created with dir name"; exit 1; }

echo "PASS"
```

- [ ] **Step 4: Create proxy-extracts-nmap.sh**

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
if [[ -n "${RT_BIN:-}" ]]; then RT="$RT_BIN"; else
    cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null
    RT="$REPO_ROOT/target/release/rt"
fi

ORIG_HOME="$HOME"
TMPDIR=$(mktemp -d)
trap 'export HOME="$ORIG_HOME"; rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"

cd "$TMPDIR"

# Use a script that cats the nmap fixture as if it were nmap output
FIXTURE="$REPO_ROOT/eval/tests/fixtures/nmap-scan.txt"
"$RT" proxy cat "$FIXTURE" > /dev/null 2>&1 || true

# cat won't trigger nmap extractor, so insert the event manually and test extraction
# Actually: we need to test that nmap tool detection + extraction works
# Better approach: create a wrapper script that pretends to be nmap
mkdir -p "$TMPDIR/bin"
cat > "$TMPDIR/bin/nmap" << 'SCRIPT'
#!/usr/bin/env bash
cat "$FIXTURE_PATH"
SCRIPT
chmod +x "$TMPDIR/bin/nmap"
export FIXTURE_PATH="$FIXTURE"
export PATH="$TMPDIR/bin:$PATH"

"$RT" proxy nmap -sV -sC -p- 10.10.10.42 > /dev/null 2>&1 || true

# Verify facts were extracted
FACTS=$("$RT" sql "SELECT COUNT(*) as count FROM facts WHERE fact_type = 'host'" --json 2>/dev/null)
echo "$FACTS" | grep -qE '"count":[1-9]' || { echo "FAIL: no host facts extracted"; exit 1; }

SERVICES=$("$RT" sql "SELECT COUNT(*) as count FROM facts WHERE fact_type = 'service'" --json 2>/dev/null)
echo "$SERVICES" | grep -qE '"count":[1-9]' || { echo "FAIL: no service facts extracted"; exit 1; }

RELATIONS=$("$RT" sql "SELECT COUNT(*) as count FROM relations" --json 2>/dev/null)
echo "$RELATIONS" | grep -qE '"count":[1-9]' || { echo "FAIL: no relations created"; exit 1; }

echo "PASS"
```

- [ ] **Step 5: Create proxy-extracts-gobuster.sh**

Same pattern as nmap but with gobuster fixture and a fake `gobuster` script.

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
if [[ -n "${RT_BIN:-}" ]]; then RT="$RT_BIN"; else
    cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null
    RT="$REPO_ROOT/target/release/rt"
fi

ORIG_HOME="$HOME"
TMPDIR=$(mktemp -d)
trap 'export HOME="$ORIG_HOME"; rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"

cd "$TMPDIR"

FIXTURE="$REPO_ROOT/eval/tests/fixtures/gobuster-scan.txt"
mkdir -p "$TMPDIR/bin"
cat > "$TMPDIR/bin/gobuster" << 'SCRIPT'
#!/usr/bin/env bash
cat "$FIXTURE_PATH"
SCRIPT
chmod +x "$TMPDIR/bin/gobuster"
export FIXTURE_PATH="$FIXTURE"
export PATH="$TMPDIR/bin:$PATH"

"$RT" proxy gobuster dir -u http://10.10.10.42 -w wordlist.txt > /dev/null 2>&1 || true

PATHS=$("$RT" sql "SELECT COUNT(*) as count FROM facts WHERE fact_type = 'web_path'" --json 2>/dev/null)
echo "$PATHS" | grep -qE '"count":[1-9]' || { echo "FAIL: no web_path facts extracted"; exit 1; }

echo "PASS"
```

- [ ] **Step 6: Create proxy-unknown-tool.sh**

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
if [[ -n "${RT_BIN:-}" ]]; then RT="$RT_BIN"; else
    cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null
    RT="$REPO_ROOT/target/release/rt"
fi

ORIG_HOME="$HOME"
TMPDIR=$(mktemp -d)
trap 'export HOME="$ORIG_HOME"; rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"

cd "$TMPDIR"

"$RT" proxy echo "unknown tool output" > /dev/null 2>&1 || true

# Event should be stored
EVENT=$("$RT" sql "SELECT COUNT(*) as count FROM events" --json 2>/dev/null)
echo "$EVENT" | grep -q '"count":1' || { echo "FAIL: event not stored"; exit 1; }

# No facts should be extracted (echo has no extractor)
FACTS=$("$RT" sql "SELECT COUNT(*) as count FROM facts" --json 2>/dev/null)
echo "$FACTS" | grep -q '"count":0' || { echo "FAIL: unexpected facts for unknown tool"; exit 1; }

echo "PASS"
```

- [ ] **Step 7: Create sql-query.sh**

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
if [[ -n "${RT_BIN:-}" ]]; then RT="$RT_BIN"; else
    cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null
    RT="$REPO_ROOT/target/release/rt"
fi

ORIG_HOME="$HOME"
TMPDIR=$(mktemp -d)
trap 'export HOME="$ORIG_HOME"; rm -rf "$TMPDIR"' EXIT
export HOME="$TMPDIR"

cd "$TMPDIR"

# Insert some data first
"$RT" proxy echo "seed" > /dev/null 2>&1 || true

# ASCII table output
TABLE=$("$RT" sql "SELECT id, command FROM events" 2>/dev/null)
echo "$TABLE" | grep -q "id" || { echo "FAIL: missing column header"; exit 1; }
echo "$TABLE" | grep -q "---" || { echo "FAIL: missing separator"; exit 1; }
echo "$TABLE" | grep -q "echo" || { echo "FAIL: missing data"; exit 1; }

# JSON output
JSON=$("$RT" sql "SELECT id, command FROM events" --json 2>/dev/null)
echo "$JSON" | grep -q '"command"' || { echo "FAIL: JSON output malformed"; exit 1; }

echo "PASS"
```

- [ ] **Step 8: Run eval tests**

Run: `bash eval/score.sh 2>&1`
Expected: All tests PASS (or most — fix any that fail).

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat: add live e2e tests for proxy, sql, and extraction workflows"
```

---

## Task 8: Implement cmd::extract (LLM path)

**Files:**
- Modify: `src/cmd/extract/mod.rs`
- Modify: `src/cmd/extract/extraction.rs`
- Modify: `src/core/agent/tools.rs`

- [ ] **Step 1: Implement agent tools for new schema**

Replace `src/core/agent/tools.rs`. Uses the `Tool { name, description, input_schema, execute: ToolExecute::new(...) }` struct literal pattern from the existing codebase:

```rust
use crate::core::db;
use aisdk::core::tools::{Tool, ToolExecute};
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use std::sync::Arc;

pub struct ToolContext {
    pub conn: Arc<std::sync::Mutex<rusqlite::Connection>>,
    pub session_id: String,
    pub event_id: i64,
}

#[derive(Deserialize, JsonSchema)]
struct QueryFactsInput {
    /// Filter by fact_type (e.g. "host", "service")
    fact_type: Option<String>,
    /// Filter by key pattern (SQL LIKE, e.g. "host:%")
    key_pattern: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct CreateFactInput {
    fact_type: String,
    key: String,
    attributes: serde_json::Value,
}

#[derive(Deserialize, JsonSchema)]
struct CreateRelationInput {
    from_key: String,
    to_key: String,
    relation_type: String,
}

pub fn make_query_facts_tool(ctx: Arc<ToolContext>) -> Tool {
    Tool {
        name: "query_facts".into(),
        description: "Query existing facts in the knowledge base. Returns JSON array of matching facts.".into(),
        input_schema: schema_for!(QueryFactsInput),
        execute: ToolExecute::new(Box::new(move |params| {
            let input: QueryFactsInput = serde_json::from_value(params)
                .map_err(|e| format!("invalid input: {e}"))?;
            let conn = ctx.conn.lock().map_err(|e| format!("db lock: {e}"))?;

            let mut sql = "SELECT key, fact_type, attributes FROM facts WHERE session_id = ?1".to_string();
            let mut bind_values: Vec<String> = vec![ctx.session_id.clone()];

            if let Some(ref ft) = input.fact_type {
                sql.push_str(" AND fact_type = ?2");
                bind_values.push(ft.clone());
            }
            if let Some(ref kp) = input.key_pattern {
                let idx = bind_values.len() + 1;
                sql.push_str(&format!(" AND key LIKE ?{idx}"));
                bind_values.push(kp.clone());
            }

            let mut stmt = conn.prepare(&sql).map_err(|e| format!("query: {e}"))?;
            let rows: Vec<serde_json::Value> = stmt
                .query_map(rusqlite::params_from_iter(&bind_values), |row| {
                    Ok(serde_json::json!({
                        "key": row.get::<_, String>(0)?,
                        "fact_type": row.get::<_, String>(1)?,
                        "attributes": row.get::<_, String>(2)?,
                    }))
                })
                .map_err(|e| format!("query: {e}"))?
                .filter_map(|r| r.ok())
                .collect();

            serde_json::to_string(&rows).map_err(|e| format!("serialize: {e}"))
        })),
    }
}

pub fn make_create_fact_tool(ctx: Arc<ToolContext>) -> Tool {
    Tool {
        name: "create_fact".into(),
        description: "Create a new fact in the knowledge base. Upserts on duplicate key (merges attributes).".into(),
        input_schema: schema_for!(CreateFactInput),
        execute: ToolExecute::new(Box::new(move |params| {
            let input: CreateFactInput = serde_json::from_value(params)
                .map_err(|e| format!("invalid input: {e}"))?;
            let conn = ctx.conn.lock().map_err(|e| format!("db lock: {e}"))?;

            db::insert_fact(
                &conn, &ctx.session_id, ctx.event_id,
                &input.fact_type, &input.key, &input.attributes,
                0.8, "llm",
            ).map_err(|e| e.to_string())?;

            serde_json::to_string(&serde_json::json!({"created": true, "key": input.key}))
                .map_err(|e| format!("serialize: {e}"))
        })),
    }
}

pub fn make_create_relation_tool(ctx: Arc<ToolContext>) -> Tool {
    Tool {
        name: "create_relation".into(),
        description: "Create a relationship between two facts. Ignored if duplicate.".into(),
        input_schema: schema_for!(CreateRelationInput),
        execute: ToolExecute::new(Box::new(move |params| {
            let input: CreateRelationInput = serde_json::from_value(params)
                .map_err(|e| format!("invalid input: {e}"))?;
            let conn = ctx.conn.lock().map_err(|e| format!("db lock: {e}"))?;

            db::insert_relation(
                &conn, &ctx.session_id,
                &input.from_key, &input.to_key, &input.relation_type,
            ).map_err(|e| e.to_string())?;

            serde_json::to_string(&serde_json::json!({"created": true}))
                .map_err(|e| format!("serialize: {e}"))
        })),
    }
}
```

- [ ] **Step 2: Implement extraction.rs**

```rust
// src/cmd/extract/extraction.rs
use crate::core::agent::{Agent, AnyModel, ToolContext};
use crate::core::agent::tools::{make_query_facts_tool, make_create_fact_tool, make_create_relation_tool};
use std::sync::{Arc, Mutex};
use std::path::PathBuf;

const MAX_EXTRACTION_ROUNDS: usize = 8;

pub fn build_system_prompt(existing_facts_summary: &str) -> String {
    format!(
        "You are an extraction agent for a knowledge base.\n\
         Parse the command output and store structured findings.\n\
         \n\
         ## Existing facts in this session\n\
         {existing_facts_summary}\n\
         \n\
         ## Fact types\n\
         - host: {{ip, hostname?, status}}, key: host:<ip>\n\
         - service: {{ip, port, protocol, service, version?}}, key: service:<ip>:<port>/<proto>\n\
         - web_path: {{ip, port, path, status_code, content_length?}}, key: web_path:<ip>:<port>:<path>\n\
         - credential: {{ip, username, password, service}}, key: credential:<user>:<service>:<ip>\n\
         - os_info: {{ip, os}}, key: os:<ip>\n\
         - Any other type you find appropriate.\n\
         \n\
         ## Relation types\n\
         runs_on, served_by, authenticates_to, describes, redirects_to, contains, exploits\n\
         \n\
         ## Instructions\n\
         - Only extract what is explicitly present. Do NOT hallucinate.\n\
         - Create records ONLY for NEW findings not already present.\n\
         - Batch all create_fact calls into a single response when possible.\n\
         - You have {MAX_EXTRACTION_ROUNDS} rounds maximum. Be efficient."
    )
}

pub async fn run_extraction(
    model: AnyModel,
    conn: Arc<Mutex<rusqlite::Connection>>,
    session_id: String,
    event_id: i64,
    command: &str,
    output: &str,
) -> Result<(), crate::error::Error> {
    // Build facts summary
    let facts_summary = {
        let c = conn.lock().unwrap();
        let mut stmt = c.prepare(
            "SELECT key, fact_type FROM facts WHERE session_id = ?1 LIMIT 100"
        ).map_err(|e| crate::error::Error::Db(e.to_string()))?;
        let rows: Vec<String> = stmt.query_map([&session_id], |row| {
            Ok(format!("{}: {}", row.get::<_, String>(1)?, row.get::<_, String>(0)?))
        }).map_err(|e| crate::error::Error::Db(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();
        if rows.is_empty() { "(none)".to_string() } else { rows.join("\n") }
    };

    let system = build_system_prompt(&facts_summary);
    let prompt = serde_json::json!({
        "task": "extract",
        "command": command,
        "output": output,
    }).to_string();

    let ctx = Arc::new(crate::core::agent::tools::ToolContext {
        conn: conn.clone(),
        session_id: session_id.clone(),
        event_id,
    });

    let tools = vec![
        make_query_facts_tool(ctx.clone()),
        make_create_fact_tool(ctx.clone()),
        make_create_relation_tool(ctx),
    ];

    let agent = Agent::new(model, system, tools, MAX_EXTRACTION_ROUNDS);
    agent.run(&prompt).await
        .map_err(|e| crate::error::Error::Config(format!("LLM extraction failed: {e}")))?;

    // Update extraction status
    {
        let c = conn.lock().unwrap();
        crate::core::db::update_extraction_status(&c, event_id, "llm_extracted")?;
    }

    Ok(())
}
```

- [ ] **Step 3: Implement cmd::extract/mod.rs**

```rust
// src/cmd/extract/mod.rs
mod extraction;

use crate::context::AppContext;
use crate::error::Error;
use redtrail::core;
use std::sync::{Arc, Mutex};

pub struct ExtractArgs {
    pub event_id: i64,
    pub force: bool,
}

pub fn run(ctx: &AppContext, args: &ExtractArgs) -> Result<(), Error> {
    // Load event
    let (command, _tool, output, status): (String, Option<String>, Option<String>, String) = ctx.conn
        .query_row(
            "SELECT command, tool, output, extraction_status FROM events WHERE id = ?1 AND session_id = ?2",
            rusqlite::params![args.event_id, ctx.session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|e| Error::Db(format!("event {} not found: {e}", args.event_id)))?;

    if !args.force && (status == "extracted" || status == "llm_extracted") {
        eprintln!("[rt] event {} already has status '{}'. Use --force to re-extract.", args.event_id, status);
        return Ok(());
    }

    let output = output.unwrap_or_default();
    if output.trim().is_empty() {
        eprintln!("[rt] event {} has empty output, skipping.", args.event_id);
        return Ok(());
    }

    let model = core::agent::create_model(&ctx.config)?;

    // We need to pass the connection to the async extraction.
    // Since AppContext owns the connection, we need to restructure slightly.
    // For now, open a second connection for the LLM agent.
    let db_path = core::db::global_db_path()?;
    let conn2 = core::db::open(db_path.to_str().unwrap())?;
    let conn_arc = Arc::new(Mutex::new(conn2));

    let rt = tokio::runtime::Runtime::new().map_err(|e| Error::Io(e))?;
    rt.block_on(extraction::run_extraction(
        model,
        conn_arc,
        ctx.session_id.clone(),
        args.event_id,
        &command,
        &output,
    ))?;

    eprintln!("[rt] LLM extraction complete for event {}", args.event_id);
    Ok(())
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo build 2>&1`
Expected: Compiles. The LLM tools API may need adaptation based on `aisdk` crate's actual Tool builder API. Adapt the pattern from the existing `src/agent/tools.rs`.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: implement extract command with LLM-based extraction agent"
```

---

## Task 9: Clean up, verify everything works

**Files:**
- Various: fix any remaining compilation issues, import paths, etc.
- Ensure all `tests/` and `eval/tests/` pass

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests PASS.

- [ ] **Step 2: Run eval tests**

Run: `bash eval/score.sh 2>&1`
Expected: All e2e tests PASS.

- [ ] **Step 3: Manual smoke test**

```bash
cargo build --release
export PATH="$PWD/target/release:$PATH"
cd /tmp && mkdir test-rt && cd test-rt
rt proxy echo "hello world"
rt sql "SELECT * FROM events"
rt sql "SELECT * FROM sessions"
rt proxy echo "another command"
rt sql "SELECT COUNT(*) FROM events"
```

Expected: events stored, sessions auto-created, SQL output formatted as ASCII table.

- [ ] **Step 4: Delete any leftover files from old codebase**

Check for orphaned files: old skills/, scripts/, docs-site/, tasks/ directories. Remove if they're not part of the new system.

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "chore: final cleanup and verification of extraction pipeline redesign"
```
