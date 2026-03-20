# Extraction Extension: Web Paths & Vulnerabilities — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `web_paths` and `vulns` tables to the KB so extraction captures directory/URL findings from gobuster/feroxbuster and vulnerability findings from nikto/nuclei.

**Architecture:** Two new DB tables with FK to `hosts`. New trait methods on `KnowledgeBase` following the exact `add_port`/`list_ports` pattern. LLM extraction prompt extended with two new JSON arrays. Two new CLI subcommands under `rt kb`. Search extended to include both new tables.

**Tech Stack:** Rust, rusqlite, clap, serde_json, reqwest (existing deps only)

**Spec:** `docs/superpowers/specs/2026-03-20-extraction-web-paths-vulns-design.md`

---

### Task 1: Schema — Add `web_paths` and `vulns` tables

**Files:**
- Modify: `src/db/mod.rs:113-127` (add tables before `chat_messages`)

- [ ] **Step 1: Add the two CREATE TABLE statements to SCHEMA**

In `src/db/mod.rs`, insert these two tables after the `notes` table (line 118) and before the `chat_messages` table (line 120):

```rust
CREATE TABLE IF NOT EXISTS web_paths (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
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

CREATE TABLE IF NOT EXISTS vulns (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
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

- [ ] **Step 2: Run `cargo build` to verify schema compiles**

Run: `cargo build 2>&1 | tail -5`
Expected: compiles with existing warnings only (no new errors)

- [ ] **Step 3: Commit**

```bash
git add src/db/mod.rs
git commit -m "feat: add web_paths and vulns tables to schema"
```

---

### Task 2: Trait methods — Extend `KnowledgeBase` trait

**Files:**
- Modify: `src/db/mod.rs:129-179` (trait definition)
- Modify: `src/db/mod.rs:299-384` (SqliteDb impl block)

- [ ] **Step 1: Add 4 new method signatures to the `KnowledgeBase` trait**

In `src/db/mod.rs`, add these after `fn list_notes` (line 175) and before `fn list_history` (line 176):

```rust
    fn add_web_path(
        &self,
        session_id: &str,
        host_ip: &str,
        port: i64,
        scheme: &str,
        path: &str,
        status_code: Option<i64>,
        content_length: Option<i64>,
        content_type: Option<&str>,
        redirect_to: Option<&str>,
        source: Option<&str>,
    ) -> Result<i64, Error>;
    fn add_vuln(
        &self,
        session_id: &str,
        host_ip: &str,
        port: i64,
        name: &str,
        severity: Option<&str>,
        cve: Option<&str>,
        url: Option<&str>,
        detail: Option<&str>,
        source: Option<&str>,
    ) -> Result<i64, Error>;
    fn list_web_paths(
        &self,
        session_id: &str,
        host_filter: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Error>;
    fn list_vulns(
        &self,
        session_id: &str,
        host_filter: Option<&str>,
        severity_filter: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Error>;
```

Note: `add_web_path` and `add_vuln` take `host_ip: &str` (not `host_id: i64`) — matching the `add_port` pattern where the IP is resolved to a host_id internally via `add_host`.

- [ ] **Step 2: Add the 4 delegation methods to `impl KnowledgeBase for SqliteDb`**

In `src/db/mod.rs`, add after the `list_notes` impl (around line 372) and before `list_history`:

```rust
    fn add_web_path(
        &self,
        session_id: &str,
        host_ip: &str,
        port: i64,
        scheme: &str,
        path: &str,
        status_code: Option<i64>,
        content_length: Option<i64>,
        content_type: Option<&str>,
        redirect_to: Option<&str>,
        source: Option<&str>,
    ) -> Result<i64, Error> {
        kb::add_web_path(
            &self.conn, session_id, host_ip, port, scheme, path,
            status_code, content_length, content_type, redirect_to, source,
        )
    }
    fn add_vuln(
        &self,
        session_id: &str,
        host_ip: &str,
        port: i64,
        name: &str,
        severity: Option<&str>,
        cve: Option<&str>,
        url: Option<&str>,
        detail: Option<&str>,
        source: Option<&str>,
    ) -> Result<i64, Error> {
        kb::add_vuln(
            &self.conn, session_id, host_ip, port, name,
            severity, cve, url, detail, source,
        )
    }
    fn list_web_paths(
        &self,
        session_id: &str,
        host_filter: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_web_paths(&self.conn, session_id, host_filter)
    }
    fn list_vulns(
        &self,
        session_id: &str,
        host_filter: Option<&str>,
        severity_filter: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Error> {
        kb::list_vulns(&self.conn, session_id, host_filter, severity_filter)
    }
```

- [ ] **Step 3: Verify it doesn't compile yet (functions don't exist in kb.rs)**

Run: `cargo build 2>&1 | grep "error"`
Expected: errors about missing `kb::add_web_path`, `kb::add_vuln`, `kb::list_web_paths`, `kb::list_vulns`

---

### Task 3: DB implementation — `add_web_path`, `add_vuln`, `list_web_paths`, `list_vulns`

**Files:**
- Modify: `src/db/kb.rs` (add functions after `add_note` at line 154, and list functions after `list_notes` at line 228)

- [ ] **Step 1: Write unit tests for the new DB functions**

In `src/db/kb.rs`, add to the existing `mod tests` block (after line 338):

```rust
    #[test]
    fn test_add_web_path_and_list() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        db.add_host("s1", "10.10.10.1", None, None).unwrap();
        let id = db.add_web_path(
            "s1", "10.10.10.1", 80, "http", "/admin",
            Some(200), Some(1234), Some("text/html"), None, Some("gobuster"),
        ).unwrap();
        assert!(id > 0);

        let paths = db.list_web_paths("s1", None).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0]["path"], "/admin");
        assert_eq!(paths[0]["status_code"], 200);
        assert_eq!(paths[0]["source"], "gobuster");
    }

    #[test]
    fn test_add_web_path_auto_creates_host() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        db.add_web_path(
            "s1", "10.10.10.99", 443, "https", "/login",
            Some(200), None, None, None, None,
        ).unwrap();

        let hosts = db.list_hosts("s1").unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0]["ip"], "10.10.10.99");
    }

    #[test]
    fn test_add_web_path_dedup() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        db.add_web_path("s1", "10.10.10.1", 80, "http", "/admin", Some(200), None, None, None, None).unwrap();
        db.add_web_path("s1", "10.10.10.1", 80, "http", "/admin", Some(403), None, None, None, None).unwrap();

        let paths = db.list_web_paths("s1", None).unwrap();
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn test_list_web_paths_host_filter() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        db.add_web_path("s1", "10.10.10.1", 80, "http", "/a", None, None, None, None, None).unwrap();
        db.add_web_path("s1", "10.10.10.2", 80, "http", "/b", None, None, None, None, None).unwrap();

        let paths = db.list_web_paths("s1", Some("10.10.10.1")).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0]["path"], "/a");
    }

    #[test]
    fn test_add_vuln_and_list() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        let id = db.add_vuln(
            "s1", "10.10.10.1", 80, "Apache Path Traversal",
            Some("high"), Some("CVE-2021-41773"),
            Some("http://10.10.10.1/cgi-bin/.."), Some("traversal"), Some("nuclei"),
        ).unwrap();
        assert!(id > 0);

        let vulns = db.list_vulns("s1", None, None).unwrap();
        assert_eq!(vulns.len(), 1);
        assert_eq!(vulns[0]["name"], "Apache Path Traversal");
        assert_eq!(vulns[0]["severity"], "high");
        assert_eq!(vulns[0]["cve"], "CVE-2021-41773");
    }

    #[test]
    fn test_add_vuln_host_level_no_port() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        db.add_vuln(
            "s1", "10.10.10.1", 0, "Outdated OS",
            Some("medium"), None, None, None, None,
        ).unwrap();

        let vulns = db.list_vulns("s1", None, None).unwrap();
        assert_eq!(vulns.len(), 1);
        assert_eq!(vulns[0]["port"], 0);
    }

    #[test]
    fn test_add_vuln_dedup() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        db.add_vuln("s1", "10.10.10.1", 80, "XSS", Some("medium"), None, None, None, None).unwrap();
        db.add_vuln("s1", "10.10.10.1", 80, "XSS", Some("high"), None, None, None, None).unwrap();

        let vulns = db.list_vulns("s1", None, None).unwrap();
        assert_eq!(vulns.len(), 1);
    }

    #[test]
    fn test_list_vulns_severity_filter() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();
        db.add_vuln("s1", "10.10.10.1", 80, "XSS", Some("medium"), None, None, None, None).unwrap();
        db.add_vuln("s1", "10.10.10.1", 443, "SQLi", Some("critical"), None, None, None, None).unwrap();

        let vulns = db.list_vulns("s1", None, Some("critical")).unwrap();
        assert_eq!(vulns.len(), 1);
        assert_eq!(vulns[0]["name"], "SQLi");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test test_add_web_path 2>&1 | tail -10`
Expected: compilation errors — functions don't exist yet

- [ ] **Step 3: Implement `add_web_path` in `src/db/kb.rs`**

Add after `add_note` function (line 154):

```rust
pub fn add_web_path(
    conn: &Connection,
    session_id: &str,
    host_ip: &str,
    port: i64,
    scheme: &str,
    path: &str,
    status_code: Option<i64>,
    content_length: Option<i64>,
    content_type: Option<&str>,
    redirect_to: Option<&str>,
    source: Option<&str>,
) -> Result<i64, Error> {
    let host_id = add_host(conn, session_id, host_ip, None, None)?;
    conn.execute(
        "INSERT OR IGNORE INTO web_paths (session_id, host_id, port, scheme, path, status_code, content_length, content_type, redirect_to, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![session_id, host_id, port, scheme, path, status_code, content_length, content_type, redirect_to, source],
    ).map_err(|e| Error::Db(e.to_string()))?;
    let id: i64 = conn
        .query_row(
            "SELECT id FROM web_paths WHERE session_id = ?1 AND host_id = ?2 AND port = ?3 AND path = ?4",
            params![session_id, host_id, port, path],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(id)
}
```

- [ ] **Step 4: Implement `add_vuln` in `src/db/kb.rs`**

Add after `add_web_path`:

```rust
pub fn add_vuln(
    conn: &Connection,
    session_id: &str,
    host_ip: &str,
    port: i64,
    name: &str,
    severity: Option<&str>,
    cve: Option<&str>,
    url: Option<&str>,
    detail: Option<&str>,
    source: Option<&str>,
) -> Result<i64, Error> {
    let host_id = add_host(conn, session_id, host_ip, None, None)?;
    conn.execute(
        "INSERT OR IGNORE INTO vulns (session_id, host_id, port, name, severity, cve, url, detail, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![session_id, host_id, port, name, severity, cve, url, detail, source],
    ).map_err(|e| Error::Db(e.to_string()))?;
    let id: i64 = conn
        .query_row(
            "SELECT id FROM vulns WHERE session_id = ?1 AND host_id = ?2 AND port = ?3 AND name = ?4",
            params![session_id, host_id, port, name],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(id)
}
```

- [ ] **Step 5: Implement `list_web_paths` in `src/db/kb.rs`**

Add after `list_notes` (line 228):

```rust
pub fn list_web_paths(
    conn: &Connection,
    session_id: &str,
    host_filter: Option<&str>,
) -> Result<Vec<serde_json::Value>, Error> {
    let map_row = |r: &rusqlite::Row<'_>| -> rusqlite::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "ip": r.get::<_, String>(0)?,
            "port": r.get::<_, i64>(1)?,
            "scheme": r.get::<_, String>(2)?,
            "path": r.get::<_, String>(3)?,
            "status_code": r.get::<_, Option<i64>>(4)?,
            "content_length": r.get::<_, Option<i64>>(5)?,
            "content_type": r.get::<_, Option<String>>(6)?,
            "redirect_to": r.get::<_, Option<String>>(7)?,
            "source": r.get::<_, Option<String>>(8)?,
        }))
    };
    if let Some(ip) = host_filter {
        let mut stmt = conn.prepare(
            "SELECT h.ip, w.port, w.scheme, w.path, w.status_code, w.content_length, w.content_type, w.redirect_to, w.source FROM web_paths w JOIN hosts h ON w.host_id = h.id WHERE w.session_id = ?1 AND h.ip = ?2 ORDER BY w.path"
        ).map_err(|e| Error::Db(e.to_string()))?;
        stmt.query_map(params![session_id, ip], map_row)
            .map_err(|e| Error::Db(e.to_string()))?
            .map(|r| r.map_err(|e| Error::Db(e.to_string())))
            .collect()
    } else {
        let mut stmt = conn.prepare(
            "SELECT h.ip, w.port, w.scheme, w.path, w.status_code, w.content_length, w.content_type, w.redirect_to, w.source FROM web_paths w JOIN hosts h ON w.host_id = h.id WHERE w.session_id = ?1 ORDER BY h.ip, w.path"
        ).map_err(|e| Error::Db(e.to_string()))?;
        stmt.query_map(params![session_id], map_row)
            .map_err(|e| Error::Db(e.to_string()))?
            .map(|r| r.map_err(|e| Error::Db(e.to_string())))
            .collect()
    }
}
```

- [ ] **Step 6: Implement `list_vulns` in `src/db/kb.rs`**

Add after `list_web_paths`:

```rust
pub fn list_vulns(
    conn: &Connection,
    session_id: &str,
    host_filter: Option<&str>,
    severity_filter: Option<&str>,
) -> Result<Vec<serde_json::Value>, Error> {
    let map_row = |r: &rusqlite::Row<'_>| -> rusqlite::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "ip": r.get::<_, String>(0)?,
            "port": r.get::<_, i64>(1)?,
            "name": r.get::<_, String>(2)?,
            "severity": r.get::<_, Option<String>>(3)?,
            "cve": r.get::<_, Option<String>>(4)?,
            "url": r.get::<_, Option<String>>(5)?,
            "detail": r.get::<_, Option<String>>(6)?,
            "source": r.get::<_, Option<String>>(7)?,
        }))
    };
    match (host_filter, severity_filter) {
        (Some(ip), Some(sev)) => {
            let mut stmt = conn.prepare(
                "SELECT h.ip, v.port, v.name, v.severity, v.cve, v.url, v.detail, v.source FROM vulns v JOIN hosts h ON v.host_id = h.id WHERE v.session_id = ?1 AND h.ip = ?2 AND v.severity = ?3 ORDER BY v.name"
            ).map_err(|e| Error::Db(e.to_string()))?;
            stmt.query_map(params![session_id, ip, sev], map_row)
                .map_err(|e| Error::Db(e.to_string()))?
                .map(|r| r.map_err(|e| Error::Db(e.to_string())))
                .collect()
        }
        (Some(ip), None) => {
            let mut stmt = conn.prepare(
                "SELECT h.ip, v.port, v.name, v.severity, v.cve, v.url, v.detail, v.source FROM vulns v JOIN hosts h ON v.host_id = h.id WHERE v.session_id = ?1 AND h.ip = ?2 ORDER BY v.name"
            ).map_err(|e| Error::Db(e.to_string()))?;
            stmt.query_map(params![session_id, ip], map_row)
                .map_err(|e| Error::Db(e.to_string()))?
                .map(|r| r.map_err(|e| Error::Db(e.to_string())))
                .collect()
        }
        (None, Some(sev)) => {
            let mut stmt = conn.prepare(
                "SELECT h.ip, v.port, v.name, v.severity, v.cve, v.url, v.detail, v.source FROM vulns v JOIN hosts h ON v.host_id = h.id WHERE v.session_id = ?1 AND v.severity = ?2 ORDER BY v.name"
            ).map_err(|e| Error::Db(e.to_string()))?;
            stmt.query_map(params![session_id, sev], map_row)
                .map_err(|e| Error::Db(e.to_string()))?
                .map(|r| r.map_err(|e| Error::Db(e.to_string())))
                .collect()
        }
        (None, None) => {
            let mut stmt = conn.prepare(
                "SELECT h.ip, v.port, v.name, v.severity, v.cve, v.url, v.detail, v.source FROM vulns v JOIN hosts h ON v.host_id = h.id WHERE v.session_id = ?1 ORDER BY h.ip, v.name"
            ).map_err(|e| Error::Db(e.to_string()))?;
            stmt.query_map(params![session_id], map_row)
                .map_err(|e| Error::Db(e.to_string()))?
                .map(|r| r.map_err(|e| Error::Db(e.to_string())))
                .collect()
        }
    }
}
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test test_add_web_path test_add_vuln test_list_web_paths test_list_vulns 2>&1 | tail -20`
Expected: all 8 new tests pass

- [ ] **Step 8: Commit**

```bash
git add src/db/mod.rs src/db/kb.rs
git commit -m "feat: add_web_path, add_vuln, list_web_paths, list_vulns DB functions"
```

---

### Task 4: Extraction — Update LLM prompt and `apply_extraction`

**Files:**
- Modify: `src/extraction.rs:36-38` (prompt string)
- Modify: `src/extraction.rs:58-173` (apply_extraction function)

- [ ] **Step 1: Write unit tests for web_paths and vulns extraction**

In `src/extraction.rs`, add to the existing `mod tests` block (after line 261):

```rust
    #[test]
    fn test_apply_extraction_web_paths() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();

        let json = r#"{"hosts":[],"ports":[],"credentials":[],"flags":[],"access":[],"web_paths":[{"ip":"10.10.10.1","port":80,"scheme":"http","path":"/admin","status_code":200,"content_length":1234,"content_type":"text/html","redirect_to":""}],"vulns":[],"notes":[]}"#;
        apply_extraction(&db, "s1", json).unwrap();

        let paths = db.list_web_paths("s1", None).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0]["path"], "/admin");
        assert_eq!(paths[0]["status_code"], 200);

        let hosts = db.list_hosts("s1").unwrap();
        assert_eq!(hosts.len(), 1, "web_path should auto-create host");
    }

    #[test]
    fn test_apply_extraction_vulns() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();

        let json = r#"{"hosts":[],"ports":[],"credentials":[],"flags":[],"access":[],"web_paths":[],"vulns":[{"ip":"10.10.10.1","port":80,"name":"Apache Path Traversal","severity":"high","cve":"CVE-2021-41773","url":"http://10.10.10.1/cgi-bin/..","detail":"traversal"}],"notes":[]}"#;
        apply_extraction(&db, "s1", json).unwrap();

        let vulns = db.list_vulns("s1", None, None).unwrap();
        assert_eq!(vulns.len(), 1);
        assert_eq!(vulns[0]["name"], "Apache Path Traversal");
        assert_eq!(vulns[0]["cve"], "CVE-2021-41773");

        let hosts = db.list_hosts("s1").unwrap();
        assert_eq!(hosts.len(), 1, "vuln should auto-create host");
    }

    #[test]
    fn test_apply_extraction_skips_invalid_web_path() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();

        let json = r#"{"hosts":[],"ports":[],"credentials":[],"flags":[],"access":[],"web_paths":[{"ip":"","port":80,"path":"/admin"},{"ip":"10.10.10.1","port":80,"path":""}],"vulns":[],"notes":[]}"#;
        apply_extraction(&db, "s1", json).unwrap();

        let paths = db.list_web_paths("s1", None).unwrap();
        assert_eq!(paths.len(), 0, "both invalid entries should be skipped");
    }

    #[test]
    fn test_apply_extraction_skips_invalid_vuln() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();

        let json = r#"{"hosts":[],"ports":[],"credentials":[],"flags":[],"access":[],"web_paths":[],"vulns":[{"ip":"...","port":80,"name":"XSS"},{"ip":"10.10.10.1","port":80,"name":""}],"notes":[]}"#;
        apply_extraction(&db, "s1", json).unwrap();

        let vulns = db.list_vulns("s1", None, None).unwrap();
        assert_eq!(vulns.len(), 0, "both invalid entries should be skipped");
    }

    #[test]
    fn test_apply_extraction_vuln_no_port_defaults_zero() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", None, None, "general").unwrap();

        let json = r#"{"hosts":[],"ports":[],"credentials":[],"flags":[],"access":[],"web_paths":[],"vulns":[{"ip":"10.10.10.1","name":"Outdated OS","severity":"medium"}],"notes":[]}"#;
        apply_extraction(&db, "s1", json).unwrap();

        let vulns = db.list_vulns("s1", None, None).unwrap();
        assert_eq!(vulns.len(), 1);
        assert_eq!(vulns[0]["port"], 0);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test test_apply_extraction_web 2>&1 | tail -10`
Expected: tests fail (no web_paths/vulns handling in apply_extraction yet)

- [ ] **Step 3: Update the LLM extraction prompt**

In `src/extraction.rs`, replace the prompt format string (line 36-38) with:

```rust
    let prompt = format!(
        "You are a pentesting data extractor. Given command output, extract structured data.\n\n\
        Command: {command}\nTool: {tool_str}\n\n\
        Output:\n{truncated}\n\n\
        Return ONLY valid JSON:\n\
        {{\"hosts\":[{{\"ip\":\"...\",\"hostname\":\"...\",\"os\":\"...\"}}],\
        \"ports\":[{{\"ip\":\"...\",\"port\":22,\"protocol\":\"tcp\",\"service\":\"ssh\",\"version\":\"...\"}}],\
        \"credentials\":[{{\"username\":\"...\",\"password\":\"...\",\"service\":\"...\",\"host\":\"...\"}}],\
        \"flags\":[{{\"value\":\"...\",\"source\":\"...\"}}],\
        \"access\":[{{\"host\":\"...\",\"user\":\"...\",\"level\":\"...\",\"method\":\"...\"}}],\
        \"web_paths\":[{{\"ip\":\"...\",\"port\":80,\"scheme\":\"http\",\"path\":\"/admin\",\"status_code\":200,\"content_length\":1234,\"content_type\":\"text/html\",\"redirect_to\":\"\"}}],\
        \"vulns\":[{{\"ip\":\"...\",\"port\":80,\"name\":\"...\",\"severity\":\"high\",\"cve\":\"CVE-...\",\"url\":\"...\",\"detail\":\"...\"}}],\
        \"notes\":[\"...\"]}}\n\n\
        Empty arrays for categories with no data found."
    );
```

- [ ] **Step 4: Add web_paths handling to `apply_extraction`**

In `src/extraction.rs`, add after the `access` block (after line 160) and before the `notes` block:

```rust
    if let Some(web_paths) = v["web_paths"].as_array() {
        for w in web_paths {
            let ip = match w["ip"].as_str() {
                Some(s) if !s.is_empty() && s != "..." => s,
                _ => continue,
            };
            let path = match w["path"].as_str() {
                Some(s) if !s.is_empty() && s != "..." => s,
                _ => continue,
            };
            let port = w["port"].as_i64().unwrap_or(80) as i64;
            let scheme = w["scheme"].as_str().unwrap_or("http");
            let status_code = w["status_code"].as_i64().map(|n| n as i64);
            let content_length = w["content_length"].as_i64();
            let content_type = w["content_type"]
                .as_str()
                .filter(|s| !s.is_empty() && *s != "...");
            let redirect_to = w["redirect_to"]
                .as_str()
                .filter(|s| !s.is_empty() && *s != "...");
            db.add_web_path(
                session_id, ip, port, scheme, path,
                status_code, content_length, content_type, redirect_to,
                Some("llm-extraction"),
            )?;
        }
    }
```

- [ ] **Step 5: Add vulns handling to `apply_extraction`**

Add after the `web_paths` block, before `notes`:

```rust
    if let Some(vulns) = v["vulns"].as_array() {
        for vl in vulns {
            let ip = match vl["ip"].as_str() {
                Some(s) if !s.is_empty() && s != "..." => s,
                _ => continue,
            };
            let name = match vl["name"].as_str() {
                Some(s) if !s.is_empty() && s != "..." => s,
                _ => continue,
            };
            let port = vl["port"].as_i64().unwrap_or(0) as i64;
            let severity = vl["severity"]
                .as_str()
                .filter(|s| !s.is_empty() && *s != "...");
            let cve = vl["cve"]
                .as_str()
                .filter(|s| !s.is_empty() && *s != "...");
            let url = vl["url"]
                .as_str()
                .filter(|s| !s.is_empty() && *s != "...");
            let detail = vl["detail"]
                .as_str()
                .filter(|s| !s.is_empty() && *s != "...");
            db.add_vuln(
                session_id, ip, port, name,
                severity, cve, url, detail,
                Some("llm-extraction"),
            )?;
        }
    }
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test extraction 2>&1 | tail -15`
Expected: all extraction tests pass (old + new)

- [ ] **Step 7: Commit**

```bash
git add src/extraction.rs
git commit -m "feat: extend LLM extraction with web_paths and vulns"
```

---

### Task 5: CLI — Add `rt kb paths` and `rt kb vulns` subcommands

**Files:**
- Modify: `src/cli/kb.rs:5-120` (KbCommands enum)
- Modify: `src/cli/kb.rs:122-353` (run function match arms)

- [ ] **Step 1: Add `Paths` and `Vulns` variants to `KbCommands` enum**

In `src/cli/kb.rs`, add after the `Notes` variant (line 40) and before `History` (line 42):

```rust
    #[command(about = "List discovered web paths and directories")]
    Paths {
        #[arg(long, help = "Output as JSON")]
        json: bool,
        #[arg(long, help = "Filter by host IP")]
        host: Option<String>,
    },
    #[command(about = "List discovered vulnerabilities")]
    Vulns {
        #[arg(long, help = "Output as JSON")]
        json: bool,
        #[arg(long, help = "Filter by host IP")]
        host: Option<String>,
        #[arg(long, help = "Filter by severity (critical, high, medium, low, info)")]
        severity: Option<String>,
    },
```

- [ ] **Step 2: Add match arms for `Paths` and `Vulns` in the `run` function**

In `src/cli/kb.rs`, add after the `Notes` match arm (after line 255) and before `History`:

```rust
        KbCommands::Paths { json, host } => {
            let rows = db.list_web_paths(session_id, host.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no paths");
            } else {
                println!(
                    "{:<18} {:<6} {:<8} {:<30} {:<6} {:<8} TYPE",
                    "IP", "PORT", "SCHEME", "PATH", "STATUS", "LENGTH"
                );
                for r in &rows {
                    println!(
                        "{:<18} {:<6} {:<8} {:<30} {:<6} {:<8} {}",
                        r["ip"].as_str().unwrap_or(""),
                        r["port"].as_i64().map(|p| p.to_string()).unwrap_or_default(),
                        r["scheme"].as_str().unwrap_or("http"),
                        r["path"].as_str().unwrap_or(""),
                        r["status_code"].as_i64().map(|s| s.to_string()).unwrap_or("-".into()),
                        r["content_length"].as_i64().map(|l| l.to_string()).unwrap_or("-".into()),
                        r["content_type"].as_str().unwrap_or("-"),
                    );
                }
            }
        }
        KbCommands::Vulns { json, host, severity } => {
            let rows = db.list_vulns(session_id, host.as_deref(), severity.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows).unwrap());
            } else if rows.is_empty() {
                println!("no vulns");
            } else {
                println!(
                    "{:<18} {:<6} {:<30} {:<10} {:<18} URL",
                    "IP", "PORT", "NAME", "SEVERITY", "CVE"
                );
                for r in &rows {
                    println!(
                        "{:<18} {:<6} {:<30} {:<10} {:<18} {}",
                        r["ip"].as_str().unwrap_or(""),
                        r["port"].as_i64().map(|p| p.to_string()).unwrap_or_default(),
                        r["name"].as_str().unwrap_or(""),
                        r["severity"].as_str().unwrap_or("-"),
                        r["cve"].as_str().unwrap_or("-"),
                        r["url"].as_str().unwrap_or("-"),
                    );
                }
            }
        }
```

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Expected: compiles with no new errors

- [ ] **Step 4: Commit**

```bash
git add src/cli/kb.rs
git commit -m "feat: add rt kb paths and rt kb vulns CLI commands"
```

---

### Task 6: Search — Extend `search` to include web_paths and vulns

**Files:**
- Modify: `src/db/kb.rs:254-292` (search function)

- [ ] **Step 1: Add web_path and vuln search queries to the `searches` array**

In `src/db/kb.rs`, in the `search` function, add two entries to the `searches` array (after the "command" entry, before the closing `]`):

```rust
        (
            "web_path",
            "SELECT 'web_path' as kind, path as value FROM web_paths w JOIN hosts h ON w.host_id = h.id WHERE w.session_id = ?1 AND (w.path LIKE ?2 OR h.ip LIKE ?2)",
        ),
        (
            "vuln",
            "SELECT 'vuln' as kind, name as value FROM vulns v JOIN hosts h ON v.host_id = h.id WHERE v.session_id = ?1 AND (v.name LIKE ?2 OR v.cve LIKE ?2 OR h.ip LIKE ?2)",
        ),
```

- [ ] **Step 2: Run existing search test to verify no regression**

Run: `cargo test test_search 2>&1 | tail -5`
Expected: pass

- [ ] **Step 3: Commit**

```bash
git add src/db/kb.rs
git commit -m "feat: extend kb search to include web_paths and vulns"
```

---

### Task 7: Integration tests

**Files:**
- Modify: `tests/kb.rs` (add integration tests)

- [ ] **Step 1: Add integration tests for paths and vulns CLI commands**

In `tests/kb.rs`, add at the end:

```rust
#[test]
fn test_add_web_path_cli_not_implemented() {
    // web_paths are populated via extraction only — no add-path CLI command
    // test the list command returns empty initially
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "paths", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[test]
fn test_vulns_cli_empty() {
    let tmp = setup_workspace();
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["kb", "vulns", "--json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 0);
}
```

- [ ] **Step 2: Run all tests**

Run: `cargo test 2>&1 | tail -10`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add tests/kb.rs
git commit -m "test: add integration tests for kb paths and vulns commands"
```

---

### Task 8: Final verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1`
Expected: all tests pass, no new warnings beyond the existing `dead_code` one

- [ ] **Step 2: Verify `rt kb --help` shows new commands**

Run: `cargo run -- kb --help 2>&1`
Expected: `paths` and `vulns` subcommands appear in the help output

- [ ] **Step 3: Run clippy**

Run: `cargo clippy 2>&1 | tail -10`
Expected: no new warnings
