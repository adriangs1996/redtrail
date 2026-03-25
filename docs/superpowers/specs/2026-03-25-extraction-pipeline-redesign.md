# Redtrail Extraction Pipeline Redesign

## Summary

Strip redtrail down to three commands (`rt proxy`, `rt sql`, `rt extract`) with a new polymorphic schema (events, facts, relations) and deterministic extraction pipeline. Remove all other commands, rigid entity tables, and unnecessary complexity.

## Philosophy

- **Events are truth. Facts are interpretation.**
- **Capture meaning, not raw text.**
- Deterministic extraction for known tools. LLM fallback as a separate command.
- Deep modules with clean interfaces. Pure functions where possible.

---

## Commands

### `rt proxy <command> [args...]`

Runs a command through a PTY, captures output, stores the event, and synchronously extracts facts if a deterministic extractor exists for the tool.

- Requires explicit invocation: `rt proxy nmap -sV 10.10.10.1` (no implicit fallback for unknown subcommands)
- Auto-creates a session from cwd if none exists
- Streams output to terminal in real-time while capturing
- After command exits: store event, run extractor, store facts, print summary

### `rt sql <query>`

Executes raw SQL against the database. Returns results formatted as an ASCII table using the default formatter. Supports both read and write operations. For SELECT queries, prints the formatted result table. For write operations (INSERT/UPDATE/DELETE), prints the number of affected rows. Invalid SQL returns an error message to stderr with exit code 1. Also supports `--json` flag to output raw JSON instead of ASCII table (useful for scripting and eval tests).

### `rt extract <event_id> [--force]`

Runs LLM-based extraction on a stored event. Uses the claude-code provider by default.

- If event already has `extraction_status` of `extracted` or `llm_extracted`, requires `--force`
- All LLM-produced facts stored with `confidence = 0.8`, `source = 'llm'`
- Updates event's `extraction_status` to `llm_extracted`

#### LLM extraction tools

The LLM agent receives three tools:

**`query_facts`** — Read existing facts for dedup context.
- Parameters: `{ session_id: String, fact_type?: String, key_pattern?: String }`
- Returns: JSON array of matching facts `[{ key, fact_type, attributes }]`

**`create_fact`** — Insert a new fact.
- Parameters: `{ fact_type: String, key: String, attributes: Object }`
- Session ID and event ID are injected automatically. Uses upsert — if key exists, merges attributes.
- Returns: `{ id: i64, created: bool }` (created=false means upsert merged)

**`create_relation`** — Insert a new relation.
- Parameters: `{ from_key: String, to_key: String, relation_type: String }`
- Session ID injected automatically. INSERT OR IGNORE on duplicate.
- Returns: `{ id: i64, created: bool }`

The system prompt includes: fact_type vocabulary, key format conventions, and a summary of existing facts in the session for dedup awareness.

---

## Migration

Clean slate. No migration from the existing schema. The old database file (`~/.redtrail/redtrail.db`) is ignored — the new system creates a fresh database. Users starting a new session get the new schema automatically.

---

## Database Schema

Four tables. No rigid entity tables (no hosts, ports, credentials tables).

```sql
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
-- Relations intentionally have no FK to facts.key. This allows inserting
-- relations before or without their target facts existing (flexible ordering).
-- Dangling references are acceptable — they represent edges to facts not yet discovered.
-- Relations carry no mutable attributes, so INSERT OR IGNORE on duplicates is
-- sufficient — no update needed when the same relation is re-discovered.

CREATE INDEX idx_events_session ON events(session_id);
CREATE INDEX idx_events_tool ON events(tool);
CREATE INDEX idx_facts_session_type ON facts(session_id, fact_type);
CREATE INDEX idx_facts_event ON facts(event_id);
CREATE INDEX idx_relations_from ON relations(session_id, from_key);
CREATE INDEX idx_relations_to ON relations(session_id, to_key);
```

### Fact key format

Content-addressable dedup keys: `{fact_type}:{identifying_fields}`

- `host:10.10.10.1`
- `service:10.10.10.1:22/tcp`
- `web_path:10.10.10.1:80:/admin`
- `credential:admin:ssh:10.10.10.1`
- `os:10.10.10.1`

### Extraction status values

- `extracted` — deterministic extractor ran, facts stored
- `stored` — no extractor available, raw event only
- `llm_extracted` — `rt extract` was run on this event
- `failed` — extractor or LLM errored

### Upsert behavior

Facts use `ON CONFLICT(session_id, key) DO UPDATE` to merge attributes when a duplicate key is inserted. Merge strategy is **shallow JSON merge** — keys in the new attributes overwrite keys in the existing attributes, but keys only present in the existing attributes are preserved. Implemented via SQLite's `json_patch(existing, new)`. Confidence uses `MAX(existing, new)` — a regex-extracted fact (1.0) is never downgraded by an LLM extraction (0.8). This handles re-running tools — new information merges into existing facts.

Concrete upsert SQL:

```sql
INSERT INTO facts (session_id, event_id, fact_type, key, attributes, confidence, source)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
ON CONFLICT(session_id, key) DO UPDATE SET
    attributes = json_patch(facts.attributes, excluded.attributes),
    confidence = MAX(facts.confidence, excluded.confidence),
    source = CASE WHEN excluded.confidence > facts.confidence THEN excluded.source ELSE facts.source END,
    event_id = excluded.event_id,
    updated_at = datetime('now');
```

---

## Architecture

### File structure

```
Cargo.toml
src/
  main.rs                       — calls cli::run()
  cli.rs                        — Clap definition + dispatch
  context.rs                    — AppContext { conn, config, session_id }
  error.rs                      — Error type
  config.rs                     — Minimal config (llm_provider, llm_model)
  core/
    mod.rs                      — pub use of core's public API
    db.rs                       — schema, open(), CRUD for events/facts/relations/sessions
    net.rs                      — IP/CIDR utilities
    extractors/
      mod.rs                    — ExtractorEntry, ExtractionResult, Fact, Relation,
                                  pub fn synthesize(), pub fn detect_tool()
      nmap.rs                   — #[inventory::submit] for ["nmap"]
      web_enum.rs               — #[inventory::submit] for ["gobuster", "ffuf", "feroxbuster", "dirb", "wfuzz"]
      hydra.rs                  — #[inventory::submit] for ["hydra"]
    fmt/
      mod.rs                    — FormatterEntry, pub fn format()
      table.rs                  — #[inventory::submit] "table" (default ASCII table)
    agent/
      mod.rs                    — AnyModel enum (ClaudeCode | Anthropic), create_model(config) -> AnyModel
      providers/
        mod.rs                  — ClaudeCodeProvider (wraps `claude` CLI subprocess)
      tools.rs                  — LLM tools (query_facts, create_fact, create_relation)
  cmd/
    mod.rs                      — pub mod proxy; pub mod sql; pub mod extract;
    proxy/
      mod.rs                    — pub fn run(ctx, args) -> Result<()>
      pty.rs                    — PTY spawn + capture (private)
    sql/
      mod.rs                    — pub fn run(ctx, args) -> Result<()>
    extract/
      mod.rs                    — pub fn run(ctx, args) -> Result<()>
      extraction.rs             — LLM agent setup (private)
tests/
  synthesize_test.rs            — test synthesize() with all tool outputs + edge cases
  db_test.rs                    — test CRUD, dedup, upsert, queries
  fmt_test.rs                   — test format() output
  fixtures/
    nmap_basic.txt
    nmap_multi_host.txt
    gobuster_basic.txt
    ffuf_basic.txt
    hydra_basic.txt
    hydra_no_results.txt
eval/
  loop.sh
  score.sh
  program.md
  weights.env
  tests/
    proxy-captures-event.sh
    proxy-extracts-nmap.sh
    proxy-extracts-gobuster.sh
    proxy-extracts-hydra.sh
    proxy-unknown-tool.sh
    proxy-auto-session.sh
    sql-query.sh
    extract-llm.sh
```

### Module boundaries

Each command is a folder with `mod.rs` exposing a single `pub fn run(ctx: &AppContext, args: &Args) -> Result<()>`. Everything else in the folder is private.

The `core` module is the shared library. Its public API:

- `core::db` — schema, connection, CRUD
- `core::net` — IP utilities
- `core::extractors::synthesize(command, tool, output) -> ExtractionResult` — the only extractor interface
- `core::extractors::detect_tool(command, tool_hint) -> Option<String>` — tool detection with prefix skipping
- `core::fmt::format(name, columns, rows) -> String` — formatter dispatch
- `core::agent` — LLM model abstraction

### Flow: `rt proxy <command>`

```
1. Auto-create session from cwd if none exists
2. Detect tool (skip env vars, sudo, proxychains, time, etc.)
3. Spawn PTY, stream output to terminal, capture full output
4. Store event in DB (command, tool, exit_code, duration, output, sha256(output))
5. Call core::extractors::synthesize(command, tool, output)
6. If ExtractionResult is non-empty:
   a. Store facts (INSERT ... ON CONFLICT UPDATE)
   b. Store relations (INSERT OR IGNORE)
   c. Update event extraction_status = 'extracted'
   d. Print summary line: "extracted 3 hosts, 7 services, 2 relations"
7. If ExtractionResult is empty:
   a. Update event extraction_status = 'stored'
```

### Auto-registration with inventory

Extractors and formatters self-register using the `inventory` crate. Adding a new extractor:

1. Create `src/core/extractors/nuclei.rs`
2. Implement the extract function
3. Add `inventory::submit! { ExtractorEntry::new(&["nuclei"], extract) }`
4. Add `mod nuclei;` in `extractors/mod.rs`

Same pattern for formatters.

```rust
// ExtractorEntry
pub struct ExtractorEntry {
    pub tools: &'static [&'static str],
    /// extract(command, output) -> ExtractionResult
    /// `command` is the full command line string (e.g., "gobuster dir -u http://10.10.10.1 -w wordlist.txt").
    /// Extractors that need target info (host, port, URL) parse it from the command args.
    pub extract: fn(&str, &str) -> ExtractionResult,
}
inventory::collect!(ExtractorEntry);

// synthesize dispatches to the right extractor
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

### Tool detection

Skips environment variable assignments (`KEY=VALUE`) and common command prefixes:

```rust
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
```

---

## Extractors

### Public interface

```rust
pub struct Fact {
    pub fact_type: String,
    pub key: String,
    pub attributes: serde_json::Value,
}

pub struct Relation {
    pub from_key: String,
    pub to_key: String,
    pub relation_type: String,
}

pub struct ExtractionResult {
    pub facts: Vec<Fact>,
    pub relations: Vec<Relation>,
}
```

`synthesize()` is a pure function. No DB, no IO, no side effects.

### Nmap extractor

Registers for: `nmap`

Parses:
- Host lines: `Nmap scan report for [hostname] (ip)` -> fact_type `host`
- Port lines: `22/tcp open ssh OpenSSH 8.9` -> fact_type `service`
- OS lines: `OS details: ...` -> fact_type `os_info`
- Relations: service `runs_on` host, os_info `describes` host

Note: masscan has a fundamentally different output format (`Discovered open port 22/tcp on 10.10.10.1`). It gets its own extractor when added — not bundled with nmap.

### Web enum extractor

Registers for: `gobuster`, `ffuf`, `feroxbuster`, `dirb`, `wfuzz`

Parses:
- Gobuster: `/path (Status: 200) [Size: 1234]`
- ffuf: `path [Status: 200, Size: 1234, ...]`
- feroxbuster: `200 GET 1234l 567w 8901c http://host/path`
- Extracts target host/port from command args (`-u http://...`)
- Relations: web_path `served_by` target service

### Hydra extractor

Registers for: `hydra`

Parses:
- Success lines: `[22][ssh] host: 10.10.10.1 login: admin password: secret`
- fact_type `credential`
- Relations: credential `authenticates_to` service

---

## Formatter

### Public interface

```rust
pub struct FormatterEntry {
    pub name: &'static str,
    pub format: fn(&[String], &[Vec<serde_json::Value>]) -> String,
}
inventory::collect!(FormatterEntry);

pub fn format(name: &str, columns: &[String], rows: &[Vec<serde_json::Value>]) -> String
```

Pure function. Default formatter is `"table"` (ASCII table).

---

## AppContext

```rust
pub struct AppContext {
    pub conn: rusqlite::Connection,
    pub config: Config,
    pub session_id: String,
}
```

Created once in `cli::run()`. Passed by reference to every command's `run()` function.

Auto-session logic: if a session already exists for the current workspace_path, reuse it. Otherwise create one named after the directory. Session ID is generated as `{directory_name}-{unix_timestamp}` (e.g., `htb-machine-1711360000`). Simple, readable, unique enough for single-user.

### Error handling

- **PTY spawn failure** (command not found, permission denied): print error to stderr, exit with code 1. No event stored.
- **Non-UTF8 output**: use `String::from_utf8_lossy` — binary output gets stored with replacement characters. Extractors may produce no facts, which is fine.
- **DB write failure after command ran**: print warning to stderr ("event not stored: {error}"). The command already executed — user saw the output. Don't silently swallow.
- **Extractor panic/error**: catch, log, set `extraction_status = 'failed'`. Event is still stored.
- **Child process killed by signal**: map to exit code 128+signal (Unix convention). Event stored normally.

### PTY sizing

The PTY inherits the user's current terminal size (query via `libc::ioctl` TIOCGWINSZ). This ensures tool output formatting matches what the user expects and avoids truncation that would break extractor regexes.

---

## Testing Strategy

### Integration tests (`tests/`)

Test through public interfaces only. No testing of private internals.

- `synthesize_test.rs` — feed real tool output fixtures to `synthesize()`, assert correct facts and relations. Covers all extractors through the single public function.
- `db_test.rs` — test event/fact/relation CRUD, upsert dedup on duplicate keys, queries via `json_extract`. Uses in-memory SQLite.
- `fmt_test.rs` — test `format()` with various column/row shapes, empty results, wide values, unicode.

### Live e2e tests (`eval/tests/`)

Shell scripts testing the compiled binary end-to-end:

- `proxy-captures-event.sh` — run `rt proxy echo hello`, verify event stored via `rt sql`
- `proxy-extracts-nmap.sh` — run `rt proxy nmap ...` (or pipe fixture), verify facts + relations
- `proxy-extracts-gobuster.sh` — same for gobuster
- `proxy-extracts-hydra.sh` — same for hydra
- `proxy-unknown-tool.sh` — run `rt proxy curl ...`, verify event stored with no facts
- `proxy-auto-session.sh` — first `rt proxy` in a directory auto-creates session
- `sql-query.sh` — `rt sql "SELECT ..."` returns formatted ASCII table
- `extract-llm.sh` — `rt extract <id>` runs LLM extraction on a stored event

---

## What Gets Deleted

### CLI commands removed
advise, ask, query, config, env, deactivate, evidence, hypothesis, ingest, init, kb, pipeline, report, scope, session, setup, skill, status

### DB tables removed
hosts, ports, credentials, access_levels, flags, hypotheses, evidence, notes, web_paths, vulns, chat_messages, global_config, session_config, command_history

### Source files removed
- `src/cli/` — all files except concept reused in cmd/proxy and cmd/sql
- `src/db/` — briefing, chat, commands, config, dispatcher, hypothesis, kb, schema, session
- `src/pipeline.rs`, `src/spawn.rs`, `src/skill_loader.rs`, `src/resolve.rs`

### What stays (adapted)
- `src/agent/` providers + model abstraction -> `src/core/agent/`
- `src/net.rs` -> `src/core/net.rs`
- `src/error.rs` — simplified
- `src/config.rs` — stripped to llm_provider + llm_model
- `eval/loop.sh`, `eval/score.sh`, `eval/program.md` — framework kept, test scripts rewritten

---

## Dependencies

### Keep
- `tokio` — async runtime (for LLM calls)
- `clap` — CLI parsing
- `serde`, `serde_json` — serialization
- `regex` — extractor parsing
- `rusqlite` (bundled) — SQLite
- `portable-pty` — PTY for proxy
- `aisdk` — LLM abstraction
- `async-trait`, `async-stream` — async support
- `tracing`, `tracing-subscriber`, `tracing-appender` — logging
- `futures` — async utilities

### Add
- `inventory` — auto-registration for extractors and formatters
- `sha2` — SHA256 hashing for event output dedup

### Remove
- `colored` — not needed with ASCII formatter
- `chrono` — use SQLite datetime()
- `uuid` — session IDs can be simpler
- `dirs` — hardcode ~/.redtrail or use simple home detection
- `schemars` — no JSON schema export
- `url` — not needed
- `toml` — no TOML config files
- `dialoguer` — no interactive prompts

### Dev dependencies (keep)
- `tempfile` — temp directories for tests
