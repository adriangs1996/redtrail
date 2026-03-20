---
title: Configuration
description: Configure Redtrail's global and workspace settings.
---

:::caution[Coming Soon]
Full configuration reference with all available options will be added in a future release.
:::

Redtrail uses a layered configuration system. Workspace settings override global defaults, giving you per-engagement control.

## Global Configuration

Global settings apply to all workspaces and are stored in `~/.config/redtrail/config.toml`. Set them with:

```bash
rt config set <key> <value>
```

Global configuration covers:

- **Default LLM provider** — which AI backend to use
- **API keys** — credentials for LLM services
- **Default target scope rules** — IP ranges to always include/exclude
- **Report templates** — default output format and branding
- **Shell integration** — prompt customization, auto-capture behavior

## Workspace Configuration

Each workspace has its own configuration stored in the workspace directory. Workspace settings override global defaults for that engagement:

```bash
# Inside a workspace
rt config set target.ip 10.10.10.1
rt config set scope.networks "10.10.10.0/24"
```

Workspace-specific options include:

- **Target definition** — IP, hostname, scope boundaries
- **Session settings** — auto-save interval, history depth
- **Ingest rules** — which tool outputs to auto-parse
- **Report metadata** — client name, engagement ID, dates

## LLM Providers

Redtrail supports multiple LLM backends for its advisor functionality:

| Provider | Description |
|----------|-------------|
| **Anthropic API** | Direct access to Claude models via API key |
| **Ollama** | Local inference with open-weight models |

Configure the active provider:

```bash
# Use Anthropic
rt config set llm.provider anthropic
rt config set llm.anthropic.api_key sk-ant-...

# Use local Ollama
rt config set llm.provider ollama
rt config set llm.ollama.model llama3
rt config set llm.ollama.url http://localhost:11434
```

Run `rt setup` for an interactive configuration wizard that walks through all options.
