---
title: CLI Reference
description: Reference for all Redtrail CLI commands.
---

:::caution[Coming Soon]
Detailed usage, flags, and examples for each command will be added in a future release. Run `rt <command> --help` for current details.
:::

Redtrail is invoked via the `rt` command. Any unrecognized command is proxied to your shell with automatic output capture.

## Commands

| Command | Aliases | Description |
|---------|---------|-------------|
| `rt init` | | Initialize a new workspace in the current directory |
| `rt kb` | | Query and manage the knowledge base (hosts, ports, creds, flags, notes) |
| `rt status` | `st` | Show session metrics: hosts, ports, creds, flags, hypotheses |
| `rt hypothesis` | `theory` | Track and manage attack hypotheses |
| `rt evidence` | `ev` | Record and manage evidence and findings |
| `rt session` | `sess` | Manage workspace sessions |
| `rt scope` | | Check whether an IP is within the defined scope |
| `rt config` | `conf` | View and modify configuration (global and workspace) |
| `rt setup` | | Run the interactive setup wizard or manage tool aliases |
| `rt ingest` | `eat` | Import tool output files into the knowledge base |
| `rt report` | `rep` | Generate a penetration test report from session data |
| `rt pipeline` | | Pipeline management (deferred to v2) |
| `rt env` | | Print shell commands to activate the redtrail environment |
| `rt deactivate` | `deact` | Print shell commands to deactivate the redtrail environment |
| `rt skill` | | Manage redtrail skills (create, test, install, remove) |
| `rt ask` | | Ask the LLM with full session context and conversation history |
| `rt query` | `q` | One-shot LLM query with session context (no history) |
| `rt sql` | | Run SQL against the redtrail database |
| `rt help` | | Print help or the help of a given subcommand |

## Shell Proxy

Any command not listed above is automatically proxied to your shell and logged in the session. Use `rt -- <cmd>` to force proxy mode for commands that share a name with a subcommand.

## Quick Reference

```bash
# Start a new engagement
rt init --target 10.10.10.1
eval "$(rt env)"

# Run tools (auto-captured)
nmap -sV 10.10.10.1

# Import results
rt ingest nmap.xml

# Track hypotheses
rt theory add "SSH allows password auth"

# Ask the advisor
rt ask "What should I try next?"

# Generate report
rt report generate
```
