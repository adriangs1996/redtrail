## Problem Statement

Redtrail's LLM interactions are hardcoded to Anthropic's tool_use API in a single file
(`ask.rs`). Extraction uses a completely separate code path with a hardcoded JSON template.
There is no abstraction for what are fundamentally agents — LLMs using tools in a loop.
Adding a new provider means rewriting the entire interaction loop. Adding a new agent means
duplicating the loop logic. The per-table write functions (`add_host`, `add_port`, etc.)
create a rigid coupling between the LLM output format and the database layer.

## Solution

Refactor all LLM interactions into **agents** — each agent is an LLM with a purpose, a set
of tools, a system prompt, and round limits. Agents share a common tool library backed by
a generic dispatcher that operates on the database and shell.

Use `aisdk` (github.com/lazy-hq/aisdk) for provider-agnostic LLM interaction, eliminating
all custom HTTP code, tool-use wire format handling, and conversation loop logic.

Replace all per-table write functions with a generic **ingest dispatcher** that handles
query/create/update for any table via column whitelisting and dynamic SQL generation.

Remove the TUI module (unused in the CLI architecture).

## User Stories

1. As a pentester using `rt proxy`, I want command outputs to be automatically extracted into structured KB data, so that I don't have to manually catalog findings.
2. As a pentester using `rt proxy`, I want to see a brief suggestion after a command produces interesting results, so that I know what to do next without explicitly asking.
3. As a pentester, I want to run `rt ask "what ports are open?"` and get an answer based on my KB data, so that I can query my findings conversationally.
4. As a pentester, I want to run `rt query "list all credentials"` and get a one-shot answer without chat history, so that I can script queries.
5. As a pentester, I want to run `rt advise` and get a full strategic analysis with hypothesis ranking and attack paths, so that I have a methodical approach to the engagement.
6. As a pentester, I want the extraction agent to query existing data before creating records, so that duplicates are avoided and existing records are updated with new information.
7. As a pentester, I want the extraction agent to handle nmap, gobuster, nuclei, and other tool outputs, so that different tool formats are parsed correctly.
8. As a pentester, I want the assistant agent to run shell commands on my behalf when I ask it to, so that I can delegate execution.
9. As a pentester, I want the strategist agent to create hypotheses based on what it observes, so that the hypothesis-driven methodology is maintained.
10. As a pentester, I want to switch between Anthropic, OpenAI, Opencode's Zen or any other supported models provider without changing my workflow, so that I can work offline or reduce costs.
11. As a pentester, I want skills to influence which tools an agent uses, so that phase-specific behavior is enforced (e.g., recon phase doesn't suggest exploits).
12. As a pentester, I want the extraction agent to be able to determine which tables it should use based on the given output, and decide if it needs to update existing data or create new records, so that the KB is always consistent and automatically maintained.
13. As a pentester, I want value constraints (e.g., severity must be info/low/medium/high/critical) enforced at the tool level, so that the LLM can't insert garbage data.
14. As a pentester, I want the `suggest` tool output to be displayed inline after command execution in `rt proxy`, so that I see hints without running a separate command.
15. As a pentester, I want the `respond` tool output from `rt ask` to be displayed as natural text, so that the interaction feels conversational. I want the output to be formatted with a markdown to ANSI converter, so that markdown formatting (e.g., code blocks) is preserved in the terminal. I want the system prompt to encourage concise answers, so that responses aren't excessively verbose.
16. As a pentester, I want extraction to be skipped for commands with no output or empty output, so that no unnecessary LLM calls are made.
17. As a pentester, I want a maximum round limit per agent, so that runaway LLM loops don't consume my API budget. And the agent should be aware of his budget and optimize its tool usage accordingly
18. As a pentester, I want tool errors to be fed back to the LLM so it can self-correct, so that minor mistakes don't kill the entire interaction. Tool errors should not count against the tool budget, to encourage experimentation and self-correction by the LLM without penalty.
19. As a pentester, I want `run_command` output processed to reduce noise and optimizing for context usage. This include removing ANSI sequences, progress bars, removing unnecessary new and chunking long outputs into multiple messages, so that the LLM can focus on the relevant information and fit it into context windows.
20. As a pentester, I want `run_command` executions logged to `command_history`, so that I have a full audit trail.
21. As a pentester, I want the system prompt to include the DB schema with value constraints, so that the LLM knows exactly what data structures are available.
22. As a pentester, I want `INSERT OR IGNORE` semantics on create, with the result indicating whether the record was new or already existed, so that the LLM can decide whether to update.
23. As a pentester, I want the LLM to never be able to write to `sessions`, `command_history`, or `chat_messages` tables, so that system-managed data is protected. This tables should not even be visible in the schema provided to the LLM.

## Implementation Decisions

### Agents

- Three agents: Extraction, Strategist, Assistant
- Each agent is defined by: tools (subset of the 6 available), system prompt, max rounds
- Agent struct with two execution methods:
  - `run()` — async, runs to completion, returns `AgentResult`. Use for Extraction and Strategist.
  - `stream()` — async, returns `AgentStream` that yields tokens as they arrive. Use for Assistant.
- Both methods build the same aisdk `LanguageModelRequest` (same tools, prompt, hooks) —
  `run()` calls `generate_text()`, `stream()` calls `stream_text()`
- aisdk's `stream_text()` handles tool calls in the background while streaming text deltas,
  so the LLM can call `query_table` (executed silently) then stream its response token by token
- `suggest` tool calls execute atomically (not streamed) — correct UX for hints
- Agents are stateless — each invocation creates a fresh agent with current session context

### Tools (6 total)

- `query_table(table, filter)` — read records, filter is key-value AND semantics
- `create_record(table, data)` — insert with INSERT OR IGNORE, returns id + created bool
- `update_record(table, id, data)` — update by id, whitelist-validated columns
- `suggest(text, priority)` — display suggestion to user
- `respond(text)` — answer user in natural language
- `run_command(command)` — shell exec with output sanitization (strip ANSI, progress bars,
  excessive newlines), chunking for long outputs, logging to command_history

### Tool Context

- `ToolContext` struct holds `Arc<Mutex<Connection>>`, `session_id: String`, `cwd: PathBuf`
- Shared across all tools in an agent via `Arc<ToolContext>`
- Tools are constructed as closures capturing the context (not via `#[tool]` macro, since
  closures need DB access)

### Dispatcher (Ingest)

- Generic dynamic SQL generation with static column whitelist per table
- 10 writable tables: hosts, ports, credentials, access_levels, flags, notes, web_paths, vulns, hypotheses, evidence
- 3 protected tables: sessions, command_history, chat_messages — not visible in schema
  provided to LLM (excluded from `as_json` output)
- ip→host_id resolution for ports, web_paths, vulns (auto-creates host if needed)
- Value constraint validation (enums, ranges) before SQL execution
- Foreign key validation (evidence.hypothesis_id must belong to current session)
- All operations scoped to current session_id (auto-injected)

### Code Deleted

- All `add_*` functions in `kb.rs` (replaced by ingest dispatcher)
- `apply_extraction` and `call_llm` in `extraction.rs` (replaced by Extraction Agent)
- `call_api`, `tool_definitions`, `execute_tool` in `ask.rs` (replaced by aisdk + agents)
- `KnowledgeBase` trait write methods, `Hypotheses` trait write methods
- `agent/llm/anthropic_api.rs` (replaced by aisdk Anthropic provider)
- Entire `tui/` module
- `blocking` reqwest feature (aisdk uses async reqwest)

### Code Kept

- `KnowledgeBase` read methods (list_hosts, list_ports, etc.) — used by CLI display and prompt building
- `Hypotheses` read methods
- `CommandLog` trait — command capture in rt proxy
- `SessionOps` trait
- `skill_loader.rs` — skills loaded same way, gains `tools` field in skill.toml
- `db/schema.rs` — enhanced with value constraints in as_json output

### Provider Configuration

- Anthropic: `aisdk::Anthropic::<DynamicModel>::builder().model_name(...).build()`
- Ollama: `aisdk::OpenAICompatible::<DynamicModel>::builder().base_url("http://localhost:11434/v1").model_name(...).build()`
- Provider selected based on config (existing `config.general.llm_model` + new `config.general.llm_provider` field)

### Async Migration

- aisdk is async (tokio). CLI entry points become `#[tokio::main] async fn main()`
- Extraction (currently spawned as blocking subprocess) uses `tokio::spawn`
- tokio already in Cargo.toml with `features = ["full"]`

### Skills Integration

- Skills remain prompt fragments loaded by phase detection
- `skill.toml` gains `tools` field to restrict available tools
- Effective tool set = intersection of agent tools and skill-declared tools
- Skills cannot add new tools — only restrict the agent's set

### JSON-Structured Inputs

- User messages to agents are structured JSON (JSON prompting pattern)
- Natural language wrapped in JSON fields, never free-form prose as top-level message
- Reinforces structured behavior from the LLM

## Testing Decisions

Good tests verify external behavior through public interfaces, not internal implementation.
A test should answer: "given this input to the agent/dispatcher, is the resulting DB state
or output correct?"

### Modules to test

1. **Ingest dispatcher** — the core module. Test every table's create/query/update path
   with in-memory SQLite. Test whitelist rejection, value constraint validation,
   ip→host_id resolution, INSERT OR IGNORE semantics, foreign key validation.
   Prior art: existing tests in `kb.rs` and `extraction.rs` (same pattern — open_in_memory,
   create session, operate, assert DB state).

2. **Agent definitions** — test that each agent registers the correct tools and builds
   the expected system prompt structure. Use a mock LanguageModel that returns canned
   tool calls. Assert DB side effects (extraction agent), output collection (suggest/respond).
   Prior art: existing `extraction::tests` module.

3. **Tool functions** — test each tool function in isolation with a real in-memory DB.
   Verify that `query_table` returns correct JSON, `create_record` handles conflicts,
   `update_record` rejects unknown columns, `run_command` captures output and logs to
   command_history.

4. **Skill loader** (modified) — test `tools` field parsing and intersection logic.
   Prior art: existing `skill_loader.rs` unit tests.

5. **Schema with constraints** — test that `as_json` output includes value constraints
   for enum/range columns. Prior art: existing `schema.rs` tests.

## Out of Scope

- Ollama-specific native API (`/api/generate`) — use OpenAI-compatible endpoint
- Delete action — append-only audit trail by design
- Raw SQL query tool — structured `query_table` replaces it
- Chat history persistence for agents — agents are stateless per invocation
  (rt ask/query already saves to chat_messages separately)
- Custom tool types defined by skills — skills restrict tools, not add them
- Web UI or API server — CLI only

## Further Notes

- The `aisdk` library is at v0.5 (pre-1.0). API may change. Pin the version in Cargo.toml.
- Tool closures are sync (`Fn(Value) -> Result<String, String>`). DB operations via
  `Arc<Mutex<Connection>>` are fine at LLM-call latency scales (seconds per round).
- The `suggest` and `respond` tools are "output" tools — they don't modify DB state,
  they produce user-visible output. With `run()`, results are collected after completion
  via step inspection. With `stream()`, text from `respond` streams token by token while
  `suggest` executes atomically as a tool call.
- Markdown-to-ANSI rendering for `respond` output (user story 15). Use a crate like
  `termimad` or `bat`'s pretty-printing for terminal markdown rendering.
- Tool budget awareness: max_rounds communicated in system prompt so agent can optimize
  tool usage. Error rounds don't count against the budget (user story 17, 18).
- Design spec: `docs/superpowers/specs/2026-03-21-action-protocol-architecture-design.md`
