# Plan: Agent Architecture

> Source PRD: `docs/prd-agent-architecture.md`
> Design Spec: `docs/superpowers/specs/2026-03-21-action-protocol-architecture-design.md`

## Architectural decisions

- **Agent struct**: `Agent { tools: Vec<Tool>, max_rounds: usize, system_prompt: String }` with `run()` (completion) and `stream()` (token streaming) methods. Both build the same aisdk `LanguageModelRequest`, differing only in `generate_text()` vs `stream_text()`.
- **aisdk**: `aisdk = { version = "0.5", features = ["anthropic", "openaichatcompletions"] }` for provider-agnostic LLM interaction. Handles tool-use loop, message assembly, provider wire format.
- **Async runtime**: tokio (already in Cargo.toml with `features = ["full"]`).
- **Provider config**: new `config.general.llm_provider` field (default "anthropic"). Combined with existing `config.general.llm_model` to construct the aisdk model.
- **Tool context**: `Arc<ToolContext>` holding `Arc<Mutex<Connection>>`, `session_id: String`, `cwd: PathBuf`. Shared across all tools in an agent. Tools constructed as closures capturing context (not `#[tool]` macro).
- **Tool input schemas**: derived via `schemars::schema_for!()` on `#[derive(Deserialize, JsonSchema)]` input structs.
- **Dispatcher**: static column whitelist per table, dynamic SQL generation, `session_id` auto-injected on create. `INSERT OR IGNORE` with `created` boolean in result. `ip → host_id` resolution for ports/web_paths/vulns.
- **Schema for LLM**: `as_json()` excludes protected tables (sessions, command_history, chat_messages), includes value constraints (enums, ranges).
- **JSON-structured inputs**: all user messages to agents are JSON. Natural language wrapped in fields, never free-form prose.
- **Three agents**: Extraction (query/create/update, 5 rounds), Strategist (query/create/update/suggest, 5 rounds), Assistant (all 6 tools, 20 rounds).

---

## Phase 1: Ingest Dispatcher

**User stories**: 6, 12, 13, 22, 23

### What to build

A generic dispatcher module that handles `query`, `create`, and `update` operations for any whitelisted table. Given a table name and data as JSON, it validates columns against the whitelist, enforces value constraints (enums, ranges), auto-injects `session_id`, resolves `ip → host_id` for joined tables, and generates parameterized SQL.

The `create` operation uses `INSERT OR IGNORE` and returns the row id plus a `created` boolean indicating whether the row was new or already existed. The `query` operation supports key-value filtering with AND semantics, including a virtual `ip` filter for tables that JOIN through hosts. The `update` operation validates that only writable columns are modified and requires an `id`.

Enhance `db/schema.rs` `as_json()` to exclude protected tables (sessions, command_history, chat_messages) and include value constraints in the output schema.

All operations are tested against in-memory SQLite with no LLM involvement.

### Acceptance criteria

- [ ] `dispatcher::create()` inserts into all 10 writable tables with correct column whitelisting
- [ ] `dispatcher::create()` rejects unknown columns and unknown tables with descriptive errors
- [ ] `dispatcher::create()` returns `{id, created: true}` for new rows, `{id, created: false}` for duplicates
- [ ] `dispatcher::create()` auto-injects `session_id` — caller never provides it
- [ ] `dispatcher::create()` resolves `ip` to `host_id` for ports, web_paths, vulns (auto-creating host)
- [ ] `dispatcher::create()` validates value constraints (enums, ranges) before SQL execution
- [ ] `dispatcher::query()` returns all columns (including id, timestamps) as JSON for any whitelisted table
- [ ] `dispatcher::query()` supports key-value filter with AND semantics
- [ ] `dispatcher::query()` resolves virtual `ip` filter for ports/web_paths/vulns via JOIN
- [ ] `dispatcher::update()` modifies only whitable columns, requires `id`, returns error for unknown columns
- [ ] `dispatcher::update()` validates value constraints
- [ ] `evidence.hypothesis_id` foreign key validated (must belong to current session)
- [ ] `as_json()` output excludes sessions, command_history, chat_messages tables
- [ ] `as_json()` output includes value constraints for enum/range columns
- [ ] All tests use in-memory SQLite, no external dependencies

---

## Phase 2: Agent Core + Extraction Agent

**User stories**: 1, 6, 7, 12, 16, 17, 18, 21

### What to build

The `Agent` struct with `run()` and `stream()` methods, `ToolContext`, and the tool constructor functions (`make_query_tool`, `make_create_tool`, `make_update_tool`). Add `aisdk` as a dependency and wire up the Anthropic provider via config.

Build the Extraction Agent: its system prompt (including DB schema with constraints, extraction-specific instructions), its tool set (query/create/update only), and its JSON-structured input format.

Wire the Extraction Agent into `rt proxy` flow, replacing the current `extraction.rs` → `call_llm` → `apply_extraction` path. The spawn mechanism (`spawn.rs` → `rt pipeline extract`) triggers the Extraction Agent instead of the old extraction function.

Skip empty/no-output commands (existing behavior preserved). Max rounds = 5, communicated in system prompt. Tool errors fed back to LLM for self-correction without counting against budget.

Demoable: run `nmap -sV <target>` through `rt proxy`, see hosts/ports populated in KB via `rt kb hosts` / `rt kb ports`.

### Acceptance criteria

- [ ] `Agent` struct exists with `run()` and `stream()` methods
- [ ] `ToolContext` with `Arc<Mutex<Connection>>` works correctly across tool closures
- [ ] `make_query_tool`, `make_create_tool`, `make_update_tool` produce valid aisdk `Tool` instances
- [ ] aisdk `LanguageModelRequest` built correctly with tools, system prompt, and `stop_when` hook
- [ ] Anthropic provider constructed from `config.general.llm_provider` + `config.general.llm_model`
- [ ] Extraction Agent system prompt includes DB schema (from `as_json()`), extraction instructions, and budget awareness
- [ ] Extraction Agent input is JSON-structured: `{"task":"extract","command":"...","tool":"...","output":"..."}`
- [ ] Empty/no-output commands skip extraction (no LLM call)
- [ ] Extraction Agent creates correct KB records from nmap output (hosts + ports)
- [ ] Extraction Agent queries existing data before creating to avoid duplicates
- [ ] Tool errors returned to LLM as error strings, LLM can self-correct
- [ ] Max 5 rounds enforced via `stop_when` hook
- [ ] `rt proxy` triggers Extraction Agent via spawn mechanism
- [ ] Agent tests use mock LanguageModel with canned tool calls + in-memory SQLite

---

## Phase 3: Assistant Agent (rt ask / rt query)

**User stories**: 3, 4, 8, 10, 15

### What to build

The Assistant Agent with all 6 tools (query/create/update/suggest/respond/run_command). Implement the `suggest` and `respond` tool constructors. The `respond` tool is the primary output mechanism — its text is displayed to the user.

Replace the current `ask.rs` implementation (`call_api`, `tool_definitions`, `execute_tool`, the Anthropic-specific tool-use loop) with the Assistant Agent using `stream()` for `rt ask` (streaming tokens to terminal) and `run()` for `rt query` (one-shot, no history).

Add markdown-to-ANSI rendering for `respond` output so code blocks, bold, etc. display correctly in the terminal. System prompt encourages concise answers.

Provider-agnostic: switching `config.general.llm_provider` to an OpenAI-compatible endpoint works without code changes.

Demoable: `rt ask "what ports are open on 10.10.10.1?"` streams a formatted answer based on KB data.

### Acceptance criteria

- [ ] `make_suggest_tool` and `make_respond_tool` produce valid aisdk `Tool` instances
- [ ] `rt ask "question"` streams response tokens to terminal via `agent.stream()`
- [ ] `rt query "question"` runs to completion via `agent.run()`, no chat history saved
- [ ] `respond` tool output rendered with markdown-to-ANSI (code blocks, bold, lists)
- [ ] `suggest` tool output displayed with priority indicator
- [ ] Assistant Agent has all 6 tools registered
- [ ] Max 20 rounds enforced
- [ ] Chat history saved for `rt ask`, not for `rt query` (existing behavior preserved)
- [ ] Switching provider in config (e.g., OpenAI-compatible) works without code changes
- [ ] System prompt includes DB schema, session context, encourages concise answers

---

## Phase 4: run_command Tool

**User stories**: 8, 19, 20

### What to build

The `run_command` tool with output sanitization and logging. When the LLM calls `run_command`, Rust executes the command via `sh -c` in the workspace directory, captures stdout+stderr, sanitizes the output (strip ANSI escape sequences, remove progress bars, collapse excessive newlines), chunks long outputs for context efficiency, logs the execution to `command_history`, and returns the result with exit code.

This tool is registered by the Assistant Agent only. The Extraction and Strategist agents cannot use it.

Demoable: `rt ask "scan 10.10.10.1 with nmap"` → agent calls run_command → output captured, sanitized, logged, and agent processes the result.

### Acceptance criteria

- [ ] `make_run_command_tool` executes `sh -c` in workspace cwd
- [ ] Captures stdout + stderr
- [ ] Strips ANSI escape sequences from output
- [ ] Removes progress bars and excessive whitespace
- [ ] Chunks output exceeding context limit (12000 chars) into manageable pieces
- [ ] Logs command to `command_history` table with exit code and duration
- [ ] Returns sanitized output + exit code to LLM
- [ ] Timeout at 300 seconds
- [ ] Only registered by Assistant Agent (not Extraction or Strategist)
- [ ] Tests verify sanitization, truncation, and command_history logging

---

## Phase 5: Strategist Agent (rt advise + hints)

**User stories**: 2, 5, 9, 14

### What to build

The Strategist Agent with two modes:

1. **Post-command hints**: triggered after extraction creates new records in `rt proxy`. Receives the new records as JSON input. May call `suggest` to display a one-liner hint, or finish silently if nothing noteworthy. Hint displayed inline below command output.

2. **Full analysis** (`rt advise`): user explicitly asks for strategic advice. Full L0-L4 reasoning, hypothesis ranking, attack path suggestions. Can create/update hypotheses via tools.

Wire post-command hints into `rt proxy` flow: after extraction agent completes, if new records were created, fire the Strategist Agent. The hint (if any) is printed to stderr below the command output.

Add `rt advise` as a CLI subcommand (or alias to existing command) that runs the Strategist Agent with a user question.

Demoable: run nmap through proxy → extraction populates KB → strategist suggests "SSH on port 22 — try password authentication" below the output. `rt advise "what next?"` gives full analysis.

### Acceptance criteria

- [ ] Strategist Agent has tools: query/create/update/suggest
- [ ] Post-command hint mode fires after extraction creates new records
- [ ] Hint input is JSON: `{"task":"advise","trigger":"new_records","new_records":[...]}`
- [ ] Hint displayed inline below command output in `rt proxy`
- [ ] No hint displayed if strategist returns no `suggest` calls
- [ ] `rt advise "question"` runs full strategic analysis
- [ ] Advise input is JSON: `{"task":"advise","trigger":"user_request","question":"..."}`
- [ ] Strategist can create/update hypotheses via tools
- [ ] Max 5 rounds enforced
- [ ] System prompt includes L0-L4 methodology, BISCL, hypothesis management instructions
- [ ] Two LLM calls per interesting command in proxy (extraction + strategist), zero for boring commands

---

## Phase 6: Skills Integration

**User stories**: 11

### What to build

Add `tools` field to `skill.toml` schema. When a skill is loaded for an agent, the effective tool set is the intersection of the agent's tools and the skill's declared tools. If a skill doesn't declare a `tools` field, all agent tools are available (backwards compatible).

Update `skill_loader.rs` to parse the `tools` field and expose it. Update agent construction to apply the intersection before building the aisdk request.

Update existing skills' `skill.toml` files with appropriate `tools` restrictions (e.g., recon skill gets query/create/run_command/suggest but not update).

Demoable: with `redtrail-recon` skill active, the agent cannot call `update_record` even if the Assistant Agent normally has it.

### Acceptance criteria

- [ ] `skill.toml` accepts optional `tools` array field
- [ ] Missing `tools` field means all agent tools available (backwards compatible)
- [ ] Effective tool set = intersection of agent tools and skill-declared tools
- [ ] Skill loader parses and exposes tools list
- [ ] Agent construction applies intersection before registering tools with aisdk
- [ ] Existing skills updated with `tools` field
- [ ] `rt skill test` validates `tools` field values against known tool names
- [ ] Tests verify intersection logic (skill restricts, skill absent, empty intersection)

---

## Phase 7: Cleanup — delete old code + TUI

**User stories**: (housekeeping)

### What to build

Remove all code paths replaced by the agent architecture:

- `kb.rs`: all `add_*` functions (add_host, add_port, add_credential, add_flag, add_access, add_note, add_web_path, add_vuln)
- `extraction.rs`: `apply_extraction`, `call_llm`, the hardcoded JSON prompt template
- `cli/ask.rs`: `call_api`, `tool_definitions`, `execute_tool`, the Anthropic tool-use loop
- `KnowledgeBase` trait: all `add_*` method signatures
- `Hypotheses` trait: `create_hypothesis`, `create_evidence` method signatures
- `agent/llm/anthropic_api.rs`: replaced by aisdk
- `tui/` module: entire directory
- `blocking` reqwest feature if no longer needed

Update trait implementations to remove deleted methods. Ensure all existing tests still pass (tests that used `add_*` functions should have been migrated to use the dispatcher in earlier phases).

### Acceptance criteria

- [ ] All `add_*` functions removed from `kb.rs`
- [ ] `apply_extraction` and `call_llm` removed from `extraction.rs`
- [ ] Old `call_api`, `tool_definitions`, `execute_tool` removed from `ask.rs`
- [ ] `KnowledgeBase` trait has only read methods
- [ ] `Hypotheses` trait has only read methods
- [ ] `tui/` module deleted
- [ ] `agent/llm/anthropic_api.rs` deleted (if still exists)
- [ ] `blocking` reqwest feature removed if unused
- [ ] All tests pass
- [ ] `cargo build` clean with no dead code warnings from deleted paths
- [ ] `cargo clippy` passes
