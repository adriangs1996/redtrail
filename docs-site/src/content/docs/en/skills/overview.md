---
title: Skills
description: Redtrail's skill system for extending functionality.
---

:::caution[Coming Soon]
Full documentation with skill development guides and API reference will be added in a future release.
:::

Skills are Redtrail's extension mechanism. They allow you to add new capabilities, automate common workflows, and share techniques with the community.

## What Are Skills?

A skill is a self-contained module that extends Redtrail's functionality. Skills can:

- Add new tool integrations and parsers
- Define automated reconnaissance workflows
- Provide specialized enumeration techniques
- Package exploit patterns for common scenarios

## Built-in Skills

Redtrail ships with a set of built-in skills covering common pentesting workflows:

| Skill | Purpose |
|-------|---------|
| `nmap-ingest` | Parse and import nmap XML/grepable output |
| `nikto-ingest` | Parse nikto scan results |
| `gobuster-ingest` | Parse directory brute-force results |
| `hydra-ingest` | Parse credential brute-force results |
| `enum-http` | Automated HTTP service enumeration |
| `enum-smb` | Automated SMB service enumeration |
| `enum-ftp` | Automated FTP service enumeration |

*The built-in list is evolving. Run `rt skill list` for the current catalog.*

## Managing Skills

```bash
# List installed skills
rt skill list

# Install a community skill
rt skill install <name>

# Remove a skill
rt skill remove <name>
```

## Custom Skill Development

You can create your own skills to automate repetitive tasks or share techniques:

```bash
# Scaffold a new skill
rt skill create my-skill
```

Custom skills are defined as structured modules with:

- **Metadata** — name, version, description, author
- **Triggers** — when the skill activates (tool output patterns, manual invocation)
- **Logic** — what the skill does (parse, extract, enrich the KB)
- **Output** — how results are stored (KB entries, notes, hypotheses)

Detailed development guides and the skill API reference will be published here once the skill format is stabilized.
