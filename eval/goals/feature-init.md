# Goal: Workspace Initialization

## Desired Outcome
The `rt init` command creates a fully functional workspace with all necessary files and configuration.

## Acceptance Criteria
The e2e test at `eval/tests/feature-init.sh` must pass. Specifically:

1. `rt init --target IP --goal GOAL --scope CIDR` succeeds (exit 0)
2. Creates `~/.redtrail/` directory with `redtrail.db` (SQLite database)
3. Session is created in database with correct target

## Current State
This feature is already implemented. This goal exists as a regression guard and as a template for new goals.

## Key Files
- `src/cli/init.rs` — init command handler
- `src/cli/mod.rs` — command routing
- `src/resolve.rs` — global context and database path resolution
- `src/db/mod.rs` — database schema and traits
