# Goal: Knowledge Base Queries

## Desired Outcome
The `rt kb` subcommands query the knowledge base and return correctly formatted JSON output.

## Acceptance Criteria
The e2e test at `eval/tests/feature-kb-query.sh` must pass. Specifically:

1. `rt kb hosts --json` returns hosts from the database
2. `rt kb ports --json` returns ports with service info
3. `rt status --json` shows correct target
4. `rt kb search TERM --json` returns matching results

## Key Files
- `src/cli/kb.rs` — kb command handler
- `src/db/kb.rs` — knowledge base queries
- `src/db/mod.rs` — database traits
