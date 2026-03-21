# Agent Architecture

Redtrail is built on **agents** — LLMs that use tools in a loop to accomplish tasks.
Each agent has a purpose, a set of tools, a system prompt, and round limits. Agents
share a common tool library backed by a dispatcher that operates on the DB and shell.

Powered by `aisdk` (github.com/lazy-hq/aisdk) for provider-agnostic LLM interaction.

## Problem

Current system is hardcoded to Anthropic's tool_use API in `ask.rs`. Adding a provider
(Ollama, OpenAI) means rewriting the entire interaction loop. Extraction uses a separate
hardcoded JSON template. No code reuse between the different LLM interactions. The code
lacks proper abstractions — what should be agents are scattered across ad-hoc functions.

## Architecture Overview

```
User (natural language)
  → CLI (rt proxy / rt ask / rt query / rt advise)
    → Agent (LLM + tools + system prompt + round limits)
      → aisdk runs the tool-use loop
        → LLM calls tools → aisdk executes → feeds results back → loops
      → Agent returns results (DB side effects, suggestions, responses)
    → Display formatted output to user
```

Three agents power redtrail:

| Agent | Purpose | Triggered by |
|-------|---------|-------------|
| **Extraction Agent** | Parse command output into structured KB data | `rt proxy` (after command capture) |
| **Strategist Agent** | Analyze state, suggest next steps, manage hypotheses | `rt advise` or post-extraction |
| **Assistant Agent** | Answer questions, run commands, interact with user | `rt ask`, `rt query` |

### Why aisdk

`aisdk` (v0.5, MIT) provides:
- `LanguageModel` trait with 60+ providers (Anthropic, OpenAI, Ollama via OpenAICompatible)
- Native tool calling abstraction — define tools once, SDK converts to provider-specific
  format (Anthropic tool_use blocks, OpenAI function calling, etc)
- Built-in agentic loop with tool execution and step tracking
- `stop_when` / `on_step_finish` hooks for round control
- Streaming support for future use

This eliminates custom protocol adapters, the tool-use loop, message assembly, and response
parsing. We only write: agents, tools, dispatcher, system prompts.

## Tools (replace Action Types)

The 7 action types become 6 tool functions. `done` is eliminated — the LLM simply
finishes its turn when it has nothing more to do (aisdk handles this natively).

### Tool Definitions

| Tool | Purpose | Parameters |
|------|---------|------------|
| `query_table` | Read records from a table | `table: String`, `filter: Option<Value>` (key-value, AND) |
| `create_record` | Insert a new record | `table: String`, `data: Value` (column values) |
| `update_record` | Update existing record by id | `table: String`, `id: i64`, `data: Value` |
| `suggest` | Display a suggestion to the user | `text: String`, `priority: String` (low/medium/high) |
| `respond` | Answer the user in natural language | `text: String` |
| `run_command` | Execute a shell command | `command: String` |

### Tool Implementation

Each tool is a function that captures a shared `ToolContext` (DB connection + session_id +
cwd) and calls the dispatcher:

```rust
struct ToolContext {
    conn: Arc<Mutex<Connection>>,
    session_id: String,
    cwd: PathBuf,
}

fn make_query_tool(ctx: Arc<ToolContext>) -> Tool {
    Tool {
        name: "query_table".into(),
        description: "Query records from a database table. Filter is key-value AND semantics.".into(),
        input_schema: schema_for_query(),
        execute: ToolExecute::new(move |input: Value| {
            let table = input["table"].as_str().ok_or("missing table")?;
            let filter = input.get("filter");
            let conn = ctx.conn.lock().map_err(|e| e.to_string())?;
            dispatcher::query(&conn, &ctx.session_id, table, filter)
                .map(|v| v.to_string())
                .map_err(|e| e.to_string())
        }),
    }
}
// Similar for create_record, update_record, run_command, suggest, respond
```

Note: aisdk tool closures are `Fn(Value) -> Result<String, String>` (sync). DB access
via `Arc<Mutex<Connection>>` since closures must be `'static + Send`.

### JSON-Structured Input Messages

User messages to the LLM are structured JSON (following JSON prompting pattern).
Natural language is wrapped in a JSON field, not sent as free-form prose.

**Extraction:**
```json
{
  "task": "extract",
  "command": "nmap -sV 10.10.10.1",
  "tool": "nmap",
  "output": "22/tcp open ssh OpenSSH 8.9p1..."
}
```

**Strategist (post-command hint):**
```json
{
  "task": "advise",
  "trigger": "new_records",
  "new_records": [
    {"table": "hosts", "id": 1, "data": {"ip": "10.10.10.1", "os": "Linux"}},
    {"table": "ports", "id": 2, "data": {"ip": "10.10.10.1", "port": 22, "service": "ssh"}}
  ]
}
```

**Strategist (rt advise):**
```json
{
  "task": "advise",
  "trigger": "user_request",
  "question": "what should I do next?"
}
```

**Ask / Query:**
```json
{
  "task": "answer",
  "question": "what ports are open on 10.10.10.1?"
}
```

### Example Flow (Extraction)

```
1. Rust builds LanguageModelRequest with extraction tools + system prompt
2. User message: {"task":"extract","command":"nmap -sV 10.10.10.1","tool":"nmap","output":"..."}
3. LLM calls query_table(table="hosts", filter={"ip":"10.10.10.1"})
4. aisdk executes tool → returns "[]" (no hosts found)
5. LLM calls create_record(table="hosts", data={"ip":"10.10.10.1","os":"Linux"})
6. aisdk executes tool → returns '{"id":1,"created":true}'
7. LLM calls create_record(table="ports", data={"ip":"10.10.10.1","port":22,...})
8. aisdk executes tool → returns '{"id":2,"created":true}'
9. LLM finishes turn (no more tool calls)
10. aisdk returns GenerateTextResponse with all steps
```

## Agents

An agent = LLM + tools + system prompt + round limits. Each agent is configured and
run via aisdk's `LanguageModelRequest`.

### Agent Definition

```rust
struct Agent {
    tools: Vec<Tool>,
    max_rounds: usize,
    system_prompt: String,
}

struct AgentResult {
    pub response: GenerateTextResponse,
}

struct AgentStream {
    pub stream: LanguageModelStream,  // yields Text, ToolCall, End chunks
}

impl Agent {
    /// Build the aisdk request (shared between run and stream).
    fn build_request(
        self,
        model: impl LanguageModel,
        input: Value,
    ) -> LanguageModelRequest {
        let mut builder = LanguageModelRequest::builder()
            .model(model)
            .system(&self.system_prompt)
            .messages(Message::builder().user(&input.to_string()).build());

        for tool in self.tools {
            builder = builder.with_tool(tool);
        }

        let max = self.max_rounds;
        builder
            .stop_when(move |opts| opts.steps().len() >= max)
            .build()
    }

    /// Run to completion. Use for Extraction and Strategist agents.
    async fn run(
        self,
        model: impl LanguageModel,
        input: Value,
    ) -> Result<AgentResult, Error> {
        let response = self.build_request(model, input)
            .generate_text()
            .await
            .map_err(|e| Error::Config(e.to_string()))?;
        Ok(AgentResult { response })
    }

    /// Stream tokens as they arrive. Use for Assistant agent (rt ask).
    /// aisdk handles tool calls in background while streaming text deltas.
    async fn stream(
        self,
        model: impl LanguageModel,
        input: Value,
    ) -> Result<AgentStream, Error> {
        let stream_response = self.build_request(model, input)
            .stream_text()
            .await
            .map_err(|e| Error::Config(e.to_string()))?;
        Ok(AgentStream { stream: stream_response.stream })
    }
}
```

### Usage: run() vs stream()

```rust
// Extraction — fire and forget, inspect DB after
let result = extraction_agent.run(model, input).await?;
// DB side effects already applied via tool calls

// Assistant — stream to terminal
let mut s = assistant_agent.stream(model, input).await?;
while let Some(chunk) = s.stream.next().await {
    match chunk {
        Text(t) => print!("{t}"),   // stream to terminal with markdown rendering
        ToolCall(_) => {},           // silent — aisdk executes tools in background
        End(_) => break,
    }
}
```
```

### Extraction Agent

Parses command output into structured knowledge base data. Fires automatically after
command capture in `rt proxy`.

- Tools: `query_table`, `create_record`, `update_record`
- Input: `{"task": "extract", "command": "...", "tool": "...", "output": "..."}`
- Max rounds: 5
- Cannot run commands, suggest, or respond — data extraction only

### Strategist Agent

Analyzes session state, suggests next steps, manages hypotheses. Two modes:

1. **Post-command hints**: fires after extraction creates new records.
   Input: `{"task": "advise", "trigger": "new_records", "new_records": [...]}`
   May call `suggest` or finish silently.
2. **Full analysis** (`rt advise`): L0-L4 reasoning, hypothesis ranking, attack paths.
   Input: `{"task": "advise", "trigger": "user_request", "question": "..."}`

- Tools: `query_table`, `create_record`, `update_record`, `suggest`
- Max rounds: 5
- Cannot run commands or respond — advisory only

### Assistant Agent

Interactive agent for the operator. Answers questions, runs commands, queries the KB,
creates records, suggests actions. Powers `rt ask` and `rt query`.

- Input: `{"task": "answer", "question": "user's natural language question"}`
- Tools: all 6
- Max rounds: 20
- Full capability — this is the interactive agent

## Dispatcher (ingest)

Provider-agnostic. Each tool function calls the dispatcher. The dispatcher handles
whitelist validation, SQL generation, transactions, and ip→host_id resolution.

### Table Whitelist

| Table | Writable Columns | Required | Resolve |
|-------|-----------------|----------|---------|
| hosts | ip, hostname, os, status | ip | — |
| ports | port, protocol, service, version | port | ip → host_id |
| credentials | username, password, hash, service, host, source | username | — |
| access_levels | host, user, level, method | host, user, level | — |
| flags | value, source | value | — |
| notes | text | text | — |
| web_paths | port, scheme, path, status_code, content_length, content_type, redirect_to, source | path | ip → host_id |
| vulns | port, name, severity, cve, url, detail, source | name | ip → host_id |
| hypotheses | statement, category, status, priority, confidence, target_component | statement, category | — |
| evidence | hypothesis_id, finding, severity, poc, raw_output | finding | — |

### Rules

- `id`, `session_id`, `host_id`, timestamps: never writable by LLM, auto-managed
- `session_id`: injected by Rust on every create
- Resolve entries: LLM sends `ip` in data, Rust resolves to `host_id` via host upsert (auto-creating if needed)
- Unknown columns/tables: tool returns error string, LLM can self-correct
- For `query_table`: all columns readable including id and timestamps, filter keys validated against column list + id
- For `update_record`: only writable columns can be SET, `id` required

### Create Conflict Strategy

`create_record` uses `INSERT OR IGNORE` (matches current behavior). The result reports
whether a row was actually inserted:

```json
{"id": 5, "created": true}
{"id": 5, "created": false}
```

When `created: false`, the existing row's id is returned. The LLM can then decide to
`update_record` if needed.

### Query Resolution for Joined Tables

Tables with `ip → host_id` resolution (ports, web_paths, vulns) support a virtual `ip`
filter key in queries. The dispatcher resolves `ip` to host_id(s) and JOINs through hosts.
Query results for these tables always include the resolved `ip` field alongside other columns.

### Foreign Key Validation

For `evidence.hypothesis_id`, the dispatcher validates the referenced hypothesis belongs
to the current session before inserting.

### Value Constraints

The schema shown to the LLM (in the system prompt) includes value constraints. These are
also enforced by the dispatcher on create/update.

| Column | Constraint |
|--------|-----------|
| `hosts.status` | enum: `up`, `down`, `unknown` |
| `hypotheses.status` | enum: `pending`, `confirmed`, `refuted` |
| `hypotheses.priority` | enum: `low`, `medium`, `high`, `critical` |
| `hypotheses.confidence` | number 0.0–1.0 |
| `evidence.severity` | enum: `info`, `low`, `medium`, `high`, `critical` |
| `vulns.severity` | enum: `info`, `low`, `medium`, `high`, `critical` |
| `ports.protocol` | enum: `tcp`, `udp` |
| `web_paths.scheme` | enum: `http`, `https` |
| `ports.port` | integer 1–65535 |
| `web_paths.port` | integer 1–65535 |
| `web_paths.status_code` | integer 100–599 |

### Tables NOT writable

`sessions`, `command_history`, `chat_messages` — system-managed only.

## run_command Behavior

- Executes via `sh -c` in the workspace directory (cwd)
- Captures stdout + stderr
- Output truncated at 12000 chars (matches current MAX_OUTPUT_CHARS)
- Timeout: 300 seconds
- Command logged to `command_history` table
- Result string includes exit code

## Code Deleted

The ingest system + aisdk replaces:

- `kb.rs`: `add_host`, `add_port`, `add_credential`, `add_flag`, `add_access`, `add_note`, `add_web_path`, `add_vuln`
- `extraction.rs`: `apply_extraction`, `call_llm`, and the hardcoded JSON template prompt
- `cli/ask.rs`: `call_api`, `tool_definitions`, `execute_tool`, the Anthropic tool-use loop
- `KnowledgeBase` trait: all `add_*` methods removed
- `Hypotheses` trait: `create_hypothesis`, `create_evidence` removed
- `agent/llm/anthropic_api.rs`: replaced by aisdk Anthropic provider
- TUI: entire `tui/` module removed (not used in CLI architecture)

### Kept

- `KnowledgeBase` read methods (`list_hosts`, `list_ports`, etc.) — used by CLI display, system prompt building
- `Hypotheses` read methods
- `CommandLog` trait — command capture in `rt proxy` is separate from the tool system
- `SessionOps` trait
- `skill_loader.rs` — skills still loaded the same way

## Intentional Exclusions

- **No `delete` tool**: pentesting benefits from append-only audit trail. Records are
  updated (e.g., hypothesis status → "refuted") but never deleted.
- **No `sql_query` tool**: the structured `query_table` tool replaces raw SQL. This reduces
  the attack surface and keeps the LLM within validated paths.

## Skills Integration

Skills remain prompt fragments loaded based on pentesting phase (via `detect_phase`).
Skills add context and instructions, NOT new tools. The 6 tools are the fixed
vocabulary. Skills influence WHAT the LLM decides to do, not what tools it CAN call.

Each skill's `skill.toml` gains a `tools` field to restrict available tools:

```toml
name = "redtrail-recon"
tools = ["query_table", "create_record", "run_command", "suggest"]
```

The effective tool set is the **intersection** of agent-allowed tools and
skill-declared tools. If a skill lists `run_command` but the agent is extraction
(which doesn't register it), `run_command` is not available.

## Protocol Compliance

With aisdk + native tool calling, compliance is significantly better than prompt-taught
JSON protocols:

### Provider side
- Native tool_use format (Anthropic, OpenAI) has higher compliance than prompt-based JSON
- Tool schemas with typed parameters reduce hallucinated fields
- aisdk handles tool_call/tool_result wire format per provider

### System prompt side
- JSON-structured input messages reinforce structured behavior
- Specialized system prompts per agent
- DB schema with value constraints (enums, ranges) shown to LLM
- Few-shot examples in system prompt for complex agents

### Rust side
- Tool functions validate all inputs (whitelist, constraints) before DB operations
- Invalid inputs return error strings — LLM sees the error and can self-correct
- `stop_when` hook caps rounds per agent
- `on_step_finish` hook can log, display, or abort

## rt proxy Flow (Updated)

```
user types command → shell executes, output captured
  → command logged to command_history
  → extraction fires (aisdk generate_text with extraction tools)
    → LLM queries existing data via query_table, creates/updates via create_record
  → if new records created:
    → strategist fires (aisdk generate_text with strategist tools)
      → maybe calls suggest tool → hint printed below command output
  → user sees command output + optional hint
```

Two LLM calls per interesting command. Zero for boring commands (no output / extraction skipped).

## Provider Configuration

```rust
// Anthropic (primary)
let model = Anthropic::<DynamicModel>::builder()
    .model_name(&config.general.llm_model)
    .build()?;

// Ollama (local, via OpenAI-compatible endpoint)
let model = OpenAICompatible::<DynamicModel>::builder()
    .base_url("http://localhost:11434/v1")
    .api_key("ollama")
    .model_name("llama3")
    .build()?;

// Both work identically with the same tools and agent definitions
```

## Async Consideration

aisdk is async (tokio). Current redtrail CLI is sync (blocking reqwest).
The migration adds `tokio` as runtime. CLI entry points become:

```rust
#[tokio::main]
async fn main() {
    // ...
}
```

Extraction (currently spawned as a blocking subprocess) can use `tokio::spawn` instead.

## File Structure (New)

```
src/
  tools/
    mod.rs          — ToolContext, make_*_tool constructors
    dispatcher.rs   — query/create/update dispatch with whitelist + dynamic SQL
    ingest.rs       — SQL generation, column validation, ip→host_id resolution
    command.rs      — run_command implementation (shell exec, output capture)
  agents/
    mod.rs          — Agent struct, Agent::run(), model factory
    extraction.rs   — Extraction Agent definition (tools + prompt)
    strategist.rs   — Strategist Agent definition (tools + prompt)
    assistant.rs    — Assistant Agent definition (tools + prompt)
  cli/
    ask.rs          — thin wrapper: build Assistant Agent, run, display
    advise.rs       — thin wrapper: build Strategist Agent, run, display
    proxy.rs        — (modified) trigger Extraction + Strategist agents via aisdk
```

## Dependencies Added

```toml
[dependencies]
aisdk = { version = "0.5", features = ["anthropic", "openaichatcompletions"] }
tokio = { version = "1", features = ["full"] }
```

`reqwest` with `blocking` feature can be removed (aisdk uses its own async reqwest).
