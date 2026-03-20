---
title: Core Concepts
description: Core concepts behind Redtrail's deductive pentesting methodology.
---

:::caution[Coming Soon]
This section is under active development. Full content with examples and diagrams will be added in a future release.
:::

Redtrail is built around a structured, deductive approach to penetration testing. Understanding these core concepts will help you get the most out of the tool.

## Workspace Model

A **workspace** is an isolated directory that holds all state for a single engagement. When you run `rt init --target <ip>`, Redtrail creates a local SQLite database, configuration files, and session tracking in the current directory. Each workspace is self-contained and portable.

## Knowledge Base (KB)

The knowledge base is Redtrail's central data store. It automatically collects and organizes:

- **Hosts** — discovered targets and their metadata
- **Ports / Services** — open ports with service identification
- **Credentials** — harvested or tested credentials
- **Flags** — captured proof-of-compromise artifacts
- **Notes** — free-form observations attached to any entity

Data enters the KB through tool output ingestion (`rt ingest`), manual entry (`rt kb`), or automatic capture from proxied commands.

## Hypotheses

Hypotheses are the core of Redtrail's methodology. Instead of ad-hoc testing, you formulate explicit attack theories and track them through their lifecycle:

1. **Proposed** — an initial theory based on observed data
2. **Testing** — actively being investigated
3. **Confirmed** — supported by evidence
4. **Refuted** — disproven, logged for the record

Use `rt hypothesis` (or `rt theory`) to create, update, and review hypotheses throughout an engagement.

## Deductive Layers (L0–L4)

Redtrail organizes the pentesting workflow into five deductive layers, each building on the previous:

| Layer | Name | Focus |
|-------|------|-------|
| **L0** | Reconnaissance | Passive and active information gathering |
| **L1** | Enumeration | Service-level probing and fingerprinting |
| **L2** | Vulnerability Analysis | Mapping findings to known weaknesses |
| **L3** | Exploitation | Attempting to confirm vulnerabilities |
| **L4** | Post-Exploitation | Privilege escalation, lateral movement, data exfiltration |

The LLM advisor uses these layers to suggest next steps appropriate to your current progress.

## BISCL Framework

**BISCL** (Breadth-first, Iterative, Structured, Contextual, Layered) is the strategic protocol that guides Redtrail's advisor:

- **Breadth-first** — enumerate the full attack surface before diving deep
- **Iterative** — revisit earlier layers as new information surfaces
- **Structured** — every action ties back to a hypothesis
- **Contextual** — suggestions consider the full KB state
- **Layered** — progress through L0→L4 methodically

Together, these concepts ensure that engagements are thorough, reproducible, and well-documented.
