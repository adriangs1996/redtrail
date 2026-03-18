# Redtrail

Pentesting orchestrator — your shell is the interface, AI skills are the methodology.

---

## Quick Start

```bash
cargo install redtrail
rt setup
cd ~/ctf/target && rt init --target 10.10.10.1
eval "$(rt env)"
nmap -sV 10.10.10.1  # proxied, captured, extracted
```

---

## How It Works

Three layers, zero friction:

1. **Your shell** (bash/zsh) — unchanged. Transparent aliases wrap your tools.
2. **`rt` CLI** — proxy captures every tool invocation, extracts findings, manages a local SQLite knowledge base per workspace.
3. **AI skills** — pentesting methodology as composable prompt modules. Skills read the KB via `rt` commands, reason over it, and emit structured next steps.

You drive. Redtrail keeps context.

---

## Workspace Model

Scoped to a directory, like Python virtualenvs.

```
~/ctf/target/
  .redtrail/
    redtrail.db    # SQLite — hosts, ports, creds, flags, history
    config.toml    # workspace-level config overrides
    aliases        # shell aliases sourced by `rt env`
```

`rt init` creates the workspace. `eval "$(rt env)"` activates it in your shell. `rt deactivate` removes aliases.

---

## Commands

### Workspace

| Command | Description |
|---|---|
| `rt init [--target IP] [--goal TYPE] [--scope CIDR]` | Create workspace in current directory |
| `rt env` | Print shell exports/aliases to eval |
| `rt deactivate` | Remove aliases from current shell |

### Knowledge Base

| Command | Description |
|---|---|
| `rt kb hosts` | List discovered hosts |
| `rt kb ports [--host IP]` | List open ports |
| `rt kb creds` | List captured credentials |
| `rt kb flags` | List captured flags |
| `rt kb access` | List shell/service access gained |
| `rt kb notes` | List manual notes |
| `rt kb history [--limit N]` | Command execution history |
| `rt kb search QUERY` | Full-text search across KB |
| `rt kb add-host IP [--os OS] [--hostname NAME]` | Manually add host |
| `rt kb add-port IP PORT [--service S] [--version V]` | Manually add port |
| `rt kb add-cred USER [--pass P] [--hash H] [--service S]` | Add credential |
| `rt kb add-flag VALUE [--source S]` | Add flag |
| `rt kb add-access HOST USER LEVEL [--method M]` | Record access gained |
| `rt kb add-note TEXT` | Add freeform note |
| `rt kb extract ID` | Re-run extraction on a history entry |

All KB read commands accept `--json` for machine-readable output.

### Reasoning

| Command | Description |
|---|---|
| `rt hypothesis create TEXT [--confidence N]` | Create a hypothesis |
| `rt hypothesis list` | List hypotheses |
| `rt hypothesis update ID --status STATUS` | Update status |
| `rt hypothesis show ID` | Show hypothesis + linked evidence |
| `rt evidence add HYPOTHESIS_ID TEXT [--source S]` | Add evidence to hypothesis |
| `rt evidence list [--hypothesis ID]` | List evidence |
| `rt evidence export [--format json]` | Export all evidence |

### Tools & Reporting

| Command | Description |
|---|---|
| `rt status [--json]` | Workspace summary (target, session, counts) |
| `rt report generate [--format md\|html]` | Generate engagement report |
| `rt ingest FILE [--tool TOOL]` | Import tool output from file |
| `rt scope check HOST` | Check if host is in scope |
| `rt config get KEY` | Read config value |
| `rt config set KEY VALUE` | Write config value |
| `rt config list` | List all config |

### Skills

| Command | Description |
|---|---|
| `rt skill list` | List installed skills |
| `rt skill init NAME` | Scaffold a new skill |
| `rt skill test PATH` | Validate skill structure |
| `rt skill install PATH` | Install skill from directory |
| `rt skill remove NAME` | Remove installed skill |

### Setup

| Command | Description |
|---|---|
| `rt setup` | Interactive first-run wizard |
| `rt setup status [--json]` | Check prerequisites |
| `rt setup aliases` | Print alias definitions |

---

## Skills

Skills are prompt modules installed to `~/.redtrail/skills/`. Each skill implements a methodology step using the KB as context.

### Built-in Skills

| Skill | Description |
|---|---|
| `recon` | Initial enumeration strategy from KB state — suggests next scans |
| `enumerate` | Deep service enumeration plan given open ports |
| `exploit` | Exploit path selection based on discovered services and CVEs |
| `privesc` | Privilege escalation checklist given current access level |
| `loot` | Post-exploitation loot and lateral movement priorities |
| `report` | Engagement narrative synthesized from full KB |

### Methodology: Synthesize → Execute → Validate

1. **Synthesize** — skill reads current KB state via `rt kb ... --json`, builds context
2. **Execute** — AI reasons over KB, emits specific tool commands and hypothesis updates
3. **Validate** — output flows back through proxy, KB updates, skill re-evaluates

### Skill Structure

```
my-skill/
  skill.toml    # metadata, triggers, dependencies
  prompt.md     # the methodology prompt
```

```toml
# skill.toml
[skill]
name = "my-skill"
version = "0.1.0"
description = "What this skill does"
author = ""

[triggers]
keywords = ["smb", "445"]

[dependencies]
commands = ["nmap"]
rt_commands = ["kb hosts", "kb ports"]
```

---

## Configuration

Global config at `~/.redtrail/config.toml`. Workspace config at `.redtrail/config.toml` overrides globals.

```toml
[general]
autonomy = "balanced"   # minimal | balanced | autonomous
auto_extract = true

[scope]
strict = false
allowed_hosts = ["10.10.10.0/24"]

[noise]
threshold = 5
filter_duplicates = true

[flags]
patterns = ['HTB\{[^}]+\}', 'FLAG\{[^}]+\}']
auto_capture = true

[tools]
aliases = ["nmap", "gobuster", "ffuf", "sqlmap", "hydra", "curl", "ssh", "nc"]

[session]
max_sessions = 10
auto_save = true
```

---

## For AI Agents

Every `rt` command is shell-invocable and supports `--json` output. Skills are portable: any agent that can run shell commands can read the KB, create hypotheses, and record evidence.

```bash
# Read current state
rt kb hosts --json
rt kb ports --json
rt status --json

# Record findings
rt kb add-cred admin --pass password123 --service ssh --host 10.10.10.1
rt hypothesis create "SMB relay possible — signing disabled" --confidence 80
rt kb add-flag "HTB{flag_value}" --source "root.txt"
```

The KB is a SQLite file at `.redtrail/redtrail.db` — agents can query it directly when `--json` output isn't sufficient.
