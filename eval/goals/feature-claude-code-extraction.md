# Goal: Claude Code Provider for Extraction Pipeline

## Desired Outcome
Redtrail can use Claude Code (`claude -p`) as its LLM backend for the extraction pipeline, using the user's Claude Code subscription instead of an Anthropic API key.

## The Problem
Currently, `create_model()` in `src/agent/mod.rs` only supports the `"anthropic"` provider, which returns `Anthropic<DynamicModel>`. The `ClaudeCodeProvider` already exists in `src/agent/providers/mod.rs` and is fully functional — it spawns `claude -p --dangerously-skip-permissions` with JSON streaming output. But it's not wired into the model creation or the extraction pipeline.

The core challenge: `create_model()` returns a concrete `Anthropic<DynamicModel>` type, but `ClaudeCodeProvider` is a different type. The extraction pipeline in `src/cli/pipeline_cmd.rs` and the agent system in `src/agent/mod.rs` need to work with either provider.

## What Needs to Change

1. **Model creation must support `claude-code` provider**: When `config.general.llm_provider == "claude-code"`, create a `ClaudeCodeProvider` instead of an `Anthropic` model. This likely requires making `create_model()` return a trait object or an enum that wraps both providers.

2. **The extraction pipeline must work with `ClaudeCodeProvider`**: `src/cli/pipeline_cmd.rs:run_extract()` calls `create_model()` and passes the result to `build_extraction_agent()`. The `Agent<M>` struct is generic over `M: LanguageModel + TextInputSupport + ToolCallSupport` — `ClaudeCodeProvider` already implements these traits.

3. **Configuration**: `rt config set general.llm_provider claude-code` should be all a user needs to switch.

## Key Files
- `src/agent/mod.rs` — `create_model()` function, `Agent<M>` struct
- `src/agent/providers/mod.rs` — `ClaudeCodeProvider` (already implemented)
- `src/cli/pipeline_cmd.rs` — `run_extract()` which uses the model
- `src/agent/extraction.rs` — `build_extraction_agent()`
- `src/agent/assistant.rs` — also uses `create_model()`, needs same fix
- `src/cli/ask.rs` — `rt ask` command, also uses `create_model()`
- `src/config.rs` — config structs

## Acceptance Criteria
The e2e test at `eval/tests/feature-claude-code-extraction.llm.sh` must pass:
1. `rt config set general.llm_provider claude-code` succeeds
2. `rt eat nmap-scan.txt` triggers extraction via claude-code provider
3. Extraction completes (status = "done")
4. Host 10.10.10.42 appears in `rt kb hosts`
5. Ports 22 (ssh) and 80 (http) appear in `rt kb ports`

## Hints
- `ClaudeCodeProvider` already implements `LanguageModel + TextInputSupport + ToolCallSupport`
- The simplest approach might be `Box<dyn LanguageModel + TextInputSupport + ToolCallSupport>` or an enum
- `Agent<M>` is generic — consider if you need dynamic dispatch or if an enum wrapper would work
- The strategist agent in `pipeline_cmd.rs` also needs the same model type, so both extraction and strategist must use the same approach
