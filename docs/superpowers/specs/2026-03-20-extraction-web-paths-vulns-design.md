# Extraction Extension: Web Paths & Vulnerabilities

## Problem

Redtrail's extraction pipeline only produces hosts, ports, credentials, flags, access levels, and notes. Output from web enumeration tools (gobuster, feroxbuster, dirbuster) and vulnerability scanners (nikto, nuclei) loses structured data — discovered paths and CVEs end up as free-text notes or are dropped entirely. This makes the KB useless for building attack paths from web discovery.

## Solution

Add two new KB entity types: **web_paths** and **vulns**. Extend the LLM extraction prompt and apply logic to populate them. Add CLI commands to query them.

## Schema

### `web_paths`

```sql
CREATE TABLE IF NOT EXISTS web_paths (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    host_id INTEGER NOT NULL REFERENCES hosts(id),
    port INTEGER NOT NULL DEFAULT 80,
    scheme TEXT NOT NULL DEFAULT 'http',
    path TEXT NOT NULL,
    status_code INTEGER,
    content_length INTEGER,
    content_type TEXT,
    redirect_to TEXT,
    source TEXT,
    found_at TEXT DEFAULT (datetime('now')),
    UNIQUE(session_id, host_id, port, path)
);
```

### `vulns`

```sql
CREATE TABLE IF NOT EXISTS vulns (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    host_id INTEGER NOT NULL REFERENCES hosts(id),
    port INTEGER NOT NULL DEFAULT 0,
    name TEXT NOT NULL,
    severity TEXT,
    cve TEXT,
    url TEXT,
    detail TEXT,
    source TEXT,
    found_at TEXT DEFAULT (datetime('now')),
    UNIQUE(session_id, host_id, port, name)
);
```

Both tables link to `hosts` via `host_id`. Auto-create host if IP not yet in KB (same pattern as ports). Unique constraints prevent duplicates across multiple ingestions of the same tool output.

**Design notes:**
- `AUTOINCREMENT` and `TEXT DEFAULT (datetime('now'))` match existing table conventions.
- `vulns.port` defaults to 0 (meaning host-level) rather than NULL, avoiding SQLite's NULL-inequality dedup problem in unique constraints.
- `scheme` on web_paths distinguishes http vs https (port alone is ambiguous for 8080, etc.).
- `source` on both tables tracks which tool produced the finding (consistent with `credentials.source`).

## LLM Extraction Prompt

Add two new arrays to the JSON schema in the extraction prompt:

```json
{
  "hosts": [...],
  "ports": [...],
  "credentials": [...],
  "flags": [...],
  "access": [...],
  "web_paths": [
    {
      "ip": "10.10.10.1",
      "port": 80,
      "scheme": "http",
      "path": "/admin",
      "status_code": 200,
      "content_length": 1234,
      "content_type": "text/html",
      "redirect_to": ""
    }
  ],
  "vulns": [
    {
      "ip": "10.10.10.1",
      "port": 80,
      "name": "Apache Path Traversal",
      "severity": "high",
      "cve": "CVE-2021-41773",
      "url": "http://10.10.10.1/cgi-bin/.%2e/.%2e/etc/passwd",
      "detail": "Directory traversal via path normalization bypass"
    }
  ],
  "notes": [...]
}
```

## Apply Logic

### `web_paths`
- Required fields: `ip` (non-empty, not "..."), `path` (non-empty), `port` (required in prompt — no silent default to 80)
- `scheme` defaults to "http" if missing
- Auto-create host from `ip` if not in KB
- Resolve `host_id` from `ip`
- `source` set from the command's tool name
- `INSERT OR IGNORE` (unique constraint handles dedup)

### `vulns`
- Required fields: `ip` (non-empty, not "..."), `name` (non-empty)
- `port` defaults to 0 in apply logic if missing (host-level vuln)
- Auto-create host from `ip` if not in KB
- Resolve `host_id` from `ip`
- Optional: `severity`, `cve`, `url`, `detail`
- `source` set from the command's tool name
- `INSERT OR IGNORE` (unique constraint handles dedup)
- Severity stored as free text (no enum validation — scanners use inconsistent labels)

## DB Trait Methods

Add to `KnowledgeBase` trait (impl on `SqliteDb` in `db/kb.rs`):

```rust
fn add_web_path(&self, session_id: &str, host_id: i64, port: i32, scheme: &str,
                path: &str, status_code: Option<i32>, content_length: Option<i64>,
                content_type: Option<&str>, redirect_to: Option<&str>,
                source: Option<&str>) -> Result<i64, Error>;

fn add_vuln(&self, session_id: &str, host_id: i64, port: i32, name: &str,
            severity: Option<&str>, cve: Option<&str>, url: Option<&str>,
            detail: Option<&str>, source: Option<&str>) -> Result<i64, Error>;

fn list_web_paths(&self, session_id: &str, host_filter: Option<&str>) -> Result<Vec<Value>, Error>;

fn list_vulns(&self, session_id: &str, host_filter: Option<&str>,
              severity_filter: Option<&str>) -> Result<Vec<Value>, Error>;
```

## CLI Commands

### `rt kb paths [--host <ip>]`
Lists discovered web paths. Output columns: `host`, `port`, `scheme`, `path`, `status`, `length`, `type`.

### `rt kb vulns [--host <ip>] [--severity <level>]`
Lists discovered vulnerabilities. Output columns: `host`, `port`, `name`, `severity`, `cve`, `url`.

## Files Changed

| File | Change |
|------|--------|
| `src/db/mod.rs` | Add CREATE TABLE for `web_paths` and `vulns`, add trait methods to `KnowledgeBase` |
| `src/db/kb.rs` | `SqliteDb` impl for add/list web_paths and vulns |
| `src/extraction.rs` | Update LLM prompt with new arrays, add apply sections for web_paths and vulns |
| `src/cli/kb.rs` | Add `paths` and `vulns` subcommands with host/severity filters, update `search` to include web_paths and vulns |
| `src/cli/mod.rs` | Register new subcommands in clap enum |
| `tests/extraction.rs` | Test extraction of web_paths and vulns from sample JSON |
| `tests/kb.rs` | Test add/list for web_paths and vulns |

## Files NOT Changed

| File | Reason |
|------|--------|
| `src/cli/ingest.rs` | Tool detection adequate for now; user will extend separately |
| `src/pipeline.rs` | No change to command capture flow |
| `src/config.rs` | No new config needed |
| `src/attack_graph.rs` | Future work to build edges from web_paths/vulns |

## Test Plan

1. Unit: `apply_extraction` with JSON containing web_paths and vulns populates DB correctly
2. Unit: Duplicate web_path (same host+port+path) is ignored on second insert
3. Unit: Duplicate vuln (same host+port+name) is ignored on second insert
4. Unit: web_path with missing IP is skipped
5. Unit: vuln with missing name is skipped
6. Unit: Auto-host-creation when web_path references unknown IP
7. Unit: web_paths JSON with no hosts array still auto-creates hosts
8. Unit: vuln with no port stores as 0 (host-level)
9. Unit: vuln with empty severity/cve stored as NULL
10. Integration: `rt kb paths` and `rt kb vulns` CLI output
11. Integration: `rt kb paths --host 10.10.10.1` filters correctly
12. Integration: `rt kb vulns --severity high` filters correctly
