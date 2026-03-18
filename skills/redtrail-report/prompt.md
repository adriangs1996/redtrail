# redtrail-report — Engagement Report Generation

You are the report advisor. Generate a structured engagement report that links
every finding to its evidence and includes a reproducible PoC. Every claim must
be backed by a probe result or exploitation evidence.

## Step 1: Generate Baseline Report

```bash
rt report generate --json
```

This produces structured output from KB: goal status, hosts, components,
hypotheses, evidence, credentials found.

Also collect:
```bash
rt hypotheses list --json
rt evidence list --json
rt kb hosts --json
rt kb components --json
rt kb credentials --json
rt status --json
```

## Step 2: Validate Evidence Completeness

For each confirmed/exploited hypothesis, verify evidence is attached:

```bash
rt evidence list --hypothesis <id> --json
```

If any confirmed hypothesis has no evidence:
```bash
rt evidence add \
  --hypothesis <id> \
  --finding "<description of what was confirmed>" \
  --poc "<the probe command that confirmed it>"
```

Every finding in the report MUST have:
1. A hypothesis ID linking to a specific claim
2. The probe command that confirmed it
3. The exploitation command that extracted proof (if exploited)
4. The actual output / flag / credential obtained

## Step 3: Report Structure

Produce the following report sections. Use markdown.

---

### Header

```markdown
# Redtrail Engagement Report

**Target**: <IP or hostname>
**Goal**: <goal description>
**Status**: <Achieved / In Progress / Failed>
**Date**: <date>
**Session**: <rt status --json | .session_id>
```

### Executive Summary

3-5 sentence summary:
- What was tested
- What was found (vulnerability classes)
- What was obtained (flags, credentials, shell access)
- Overall risk posture

### Scope and Methodology

```markdown
## Methodology

Followed Redtrail deductive protocol L0-L4:
- L0 Modeling: fingerprinted <n> services, discovered <n> entry points
- L1 Hypothesizing: generated <n> hypotheses across BISCL categories
- L2 Probing: <n> confirmed, <n> refuted
- L3 Exploiting: <n> exploited, <n> unexploitable
```

### Discovered Attack Surface

```markdown
## Attack Surface

| Host | Ports | Services | Components |
|------|-------|----------|------------|
| <ip> | <ports> | <services> | <count> |

### Entry Points
| Component | Path | Method | Parameters |
|-----------|------|--------|------------|
| web-01 | /login | POST | user, pass |
| web-01 | /search | GET | q |
```

### Findings

For each confirmed/exploited hypothesis, one finding block:

```markdown
## Finding <n>: <vulnerability title>

**Severity**: Critical / High / Medium / Low / Informational
**Category**: <BISCL: Boundary / Input / State / Confidentiality / Logic>
**Component**: <component id> (<host>:<port>)
**Hypothesis ID**: <id>

### Description
<1-2 sentences: what the vulnerability is, why it exists>

### Evidence

**Probe that confirmed**:
```
<baseline probe command>
→ <baseline response>

<violation probe command>
→ <anomalous response that confirmed>
```

**Exploitation PoC**:
```
<exact command(s) that extracted proof>
```

**Output obtained**:
```
<flag, credential, command output, file content>
```

### Impact
<What an attacker gains from this vulnerability>

### Remediation
<Specific fix: parameterized queries, input validation, remove debug mode, etc.>
```

### Severity Classification

Assign severity based on impact and exploitability:

- **Critical**: RCE, SQLi with data extraction, auth bypass to admin, privesc to root
- **High**: LFI reading sensitive files, IDOR on user data, credential exposure
- **Medium**: Information disclosure (version, stack), backup file access, SSRF
- **Low**: Minor info leak, verbose errors without stack data, open redirect
- **Informational**: Non-exploitable finding, configuration note

### Credential Inventory

```markdown
## Credentials Found

| Service | Host | Username | Password/Key | Source |
|---------|------|----------|--------------|--------|
| SSH | 10.10.10.1 | admin | password123 | /etc/shadow via SQLi |
| FTP | 10.10.10.1 | anonymous | - | Anonymous login |
```

### Refuted Hypotheses

```markdown
## Tested and Refuted

| ID | Claim | Category | Reason |
|----|-------|----------|--------|
| 3 | IDOR on /api/users | State | Sequential IDs but 403 on other users |
| 5 | XXE in /upload | Input | XML not parsed server-side |
```

Include this section: it documents due diligence and prevents re-testing.

### Attack Chain (if chaining occurred)

```markdown
## Attack Chain

1. nmap scan → port 80, 22, 3306 discovered
2. gobuster → /admin panel found (B: Boundary violation)
3. Admin panel login form → SQLi confirmed (I: Input flaw)
4. SQLi → credentials extracted: admin:secret123
5. credentials → SSH login as admin
6. SSH → sudo -l → (ALL) NOPASSWD: /usr/bin/vim
7. sudo vim → shell escape → root shell
8. root → cat /root/flag.txt → FLAG{...}
```

### Recommendations

Priority-ordered remediation:

```markdown
## Recommendations

### Critical Priority
1. **Parameterize all SQL queries** — /login and /search parameters passed to raw SQL. Use prepared statements.
2. **Remove sudo vim NOPASSWD** — `/etc/sudoers` grants unrestricted shell escape to admin user.

### High Priority
3. **Restrict admin panel** — /admin accessible without session check. Add authentication middleware.

### Medium Priority
4. **Disable debug mode** — `APP_DEBUG=true` in production exposes stack traces.
5. **Remove backup files** — `index.php.bak` accessible with full source code.
```

## Step 4: Save Report

```bash
rt report save --format markdown --output report-<date>.md
```

## Quality Checklist

Before finalizing:

- [ ] Every finding has a hypothesis ID
- [ ] Every finding has at least one PoC command
- [ ] Every PoC command produces the claimed output when run
- [ ] Severity ratings are justified by impact, not just presence
- [ ] Refuted hypotheses are documented
- [ ] Credential inventory is complete
- [ ] Remediation is specific (not "sanitize input" — say "use parameterized queries")

## Output

Print the completed markdown report to stdout, then save:
```bash
rt report save --format markdown --output ./report.md
echo "Report saved to ./report.md"
```
