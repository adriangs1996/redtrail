---
title: Contributing
description: How to contribute to Redtrail.
---

:::caution[Coming Soon]
Full contributing guidelines, code style documentation, and development setup instructions will be added in a future release.
:::

Redtrail is open source and welcomes contributions. This section will guide you through the project's architecture and development workflow.

## Architecture Overview

Redtrail is a Rust application organized into several key modules:

| Module | Purpose |
|--------|---------|
| `tui/` | Terminal UI — block-based shell with modal panels |
| `agent/` | LLM integration — driver loop, providers, strategist |
| `knowledge.rs` | Knowledge base — hosts, ports, creds, flags, notes |
| `attack_graph.rs` | Attack graph construction and traversal |
| `strategist.rs` | BISCL deductive protocol (L0–L4) |
| `reactor.rs` | Event processing and state transitions |
| `db.rs` | SQLite database layer |
| `report/` | Report generation (Markdown, PDF) |
| `flags.rs` | Flag capture and tracking |
| `types.rs` | Shared type definitions |
| `error.rs` | Error types and handling |

## Getting Started

```bash
# Clone the repository
git clone https://github.com/user/redtrail.git
cd redtrail

# Build
cargo build

# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run
```

## Areas for Contribution

- **Tool parsers** — add ingest support for new pentesting tools
- **Skills** — create and share reusable skill modules
- **Report templates** — additional output formats and styles
- **Documentation** — improve guides, tutorials, and API docs
- **Bug fixes** — check the issue tracker for open bugs

Detailed contribution guidelines, PR process, and code style documentation will be published here as the project matures.
