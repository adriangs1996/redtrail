use crate::error::Error;
use rusqlite::{Connection, params};

pub fn active_session_id(conn: &Connection, workspace_path: &str) -> Result<String, Error> {
    conn.query_row(
        "SELECT id FROM sessions WHERE workspace_path = ?1 AND active = 1",
        params![workspace_path],
        |r| r.get(0),
    )
    .map_err(|_| Error::NoActiveSession)
}

pub fn load_flag_patterns(conn: &Connection, session_id: &str) -> Result<Vec<String>, Error> {
    let meta: Option<String> = conn
        .query_row(
            "SELECT goal_meta FROM sessions WHERE id = ?1",
            params![session_id],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    if let Some(m) = meta
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&m)
        && let Some(arr) = v.get("flag_patterns").and_then(|x| x.as_array())
    {
        let pats: Vec<String> = arr
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect();
        if !pats.is_empty() {
            return Ok(pats);
        }
    }

    Ok(vec![
        r"HTB\{[^}]+\}".to_string(),
        r"FLAG\{[^}]+\}".to_string(),
        r"flag\{[^}]+\}".to_string(),
    ])
}

pub fn load_scope(conn: &Connection, session_id: &str) -> Result<Option<String>, Error> {
    let scope: Option<String> = conn
        .query_row(
            "SELECT scope FROM sessions WHERE id = ?1",
            params![session_id],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(scope.filter(|s| !s.is_empty()))
}

pub fn decrement_noise_budget(conn: &Connection, session_id: &str, cost: f64) -> Result<(), Error> {
    conn.execute(
        "UPDATE sessions SET noise_budget = max(0, noise_budget - ?1) WHERE id = ?2",
        params![cost, session_id],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn create_session(
    conn: &Connection,
    id: &str,
    name: &str,
    workspace_path: &str,
    target: Option<&str>,
    scope: Option<&str>,
    goal: &str,
) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO sessions (id, name, workspace_path, target, scope, goal) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, name, workspace_path, target, scope, goal],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn deactivate_session(conn: &Connection, workspace_path: &str) -> Result<(), Error> {
    conn.execute(
        "UPDATE sessions SET active = 0 WHERE workspace_path = ?1 AND active = 1",
        params![workspace_path],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn activate_session(conn: &Connection, session_id: &str) -> Result<(), Error> {
    conn.execute(
        "UPDATE sessions SET active = 1 WHERE id = ?1",
        params![session_id],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn find_session_by_name_or_id(
    conn: &Connection,
    name_or_id: &str,
    workspace_path: &str,
) -> Result<serde_json::Value, Error> {
    conn.query_row(
        "SELECT id, name, workspace_path, target, scope, goal, phase, noise_budget, active, created_at, updated_at FROM sessions WHERE workspace_path = ?2 AND (id = ?1 OR name = ?1)",
        params![name_or_id, workspace_path],
        |r| Ok(serde_json::json!({
            "id": r.get::<_, String>(0)?,
            "name": r.get::<_, String>(1)?,
            "workspace_path": r.get::<_, String>(2)?,
            "target": r.get::<_, Option<String>>(3)?,
            "scope": r.get::<_, Option<String>>(4)?,
            "goal": r.get::<_, String>(5)?,
            "phase": r.get::<_, String>(6)?,
            "noise_budget": r.get::<_, f64>(7)?,
            "active": r.get::<_, i64>(8)?,
            "created_at": r.get::<_, String>(9)?,
            "updated_at": r.get::<_, String>(10)?,
        })),
    ).map_err(|_| Error::Db(format!("session '{}' not found in this workspace", name_or_id)))
}

pub fn list_sessions(conn: &Connection, workspace_path: &str) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, workspace_path, active, target, scope, goal, phase, created_at FROM sessions WHERE workspace_path = ? ORDER BY created_at DESC"
    ).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt.query_map(params![workspace_path], |r| {
        Ok(serde_json::json!({
            "id": r.get::<_, String>(0)?,
            "name": r.get::<_, String>(1)?,
            "workspace_path": r.get::<_, String>(2)?,
            "active": r.get::<_, i64>(3)?,
            "target": r.get::<_, Option<String>>(4)?,
            "scope": r.get::<_, Option<String>>(5)?,
            "goal": r.get::<_, String>(6)?,
            "phase": r.get::<_, String>(7)?,
            "created_at": r.get::<_, String>(8)?,
        }))
    }).map_err(|e| Error::Db(e.to_string()))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| Error::Db(e.to_string()))
}

pub fn list_all_sessions(conn: &Connection) -> Result<Vec<serde_json::Value>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, workspace_path, active, target, scope, goal, phase, created_at FROM sessions ORDER BY workspace_path, created_at DESC"
    ).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt.query_map([], |r| {
        Ok(serde_json::json!({
            "id": r.get::<_, String>(0)?,
            "name": r.get::<_, String>(1)?,
            "workspace_path": r.get::<_, String>(2)?,
            "active": r.get::<_, i64>(3)?,
            "target": r.get::<_, Option<String>>(4)?,
            "scope": r.get::<_, Option<String>>(5)?,
            "goal": r.get::<_, String>(6)?,
            "phase": r.get::<_, String>(7)?,
            "created_at": r.get::<_, String>(8)?,
        }))
    }).map_err(|e| Error::Db(e.to_string()))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| Error::Db(e.to_string()))
}

pub fn get_session(conn: &Connection, session_id: &str) -> Result<serde_json::Value, Error> {
    conn.query_row(
        "SELECT id, name, workspace_path, target, scope, goal, phase, noise_budget, active, created_at, updated_at FROM sessions WHERE id = ?1",
        params![session_id],
        |r| Ok(serde_json::json!({
            "id": r.get::<_, String>(0)?,
            "name": r.get::<_, String>(1)?,
            "workspace_path": r.get::<_, String>(2)?,
            "target": r.get::<_, Option<String>>(3)?,
            "scope": r.get::<_, Option<String>>(4)?,
            "goal": r.get::<_, String>(5)?,
            "phase": r.get::<_, String>(6)?,
            "noise_budget": r.get::<_, f64>(7)?,
            "active": r.get::<_, i64>(8)?,
            "created_at": r.get::<_, String>(9)?,
            "updated_at": r.get::<_, String>(10)?,
        })),
    ).map_err(|e| Error::Db(e.to_string()))
}

pub fn status_summary(conn: &Connection, session_id: &str) -> Result<serde_json::Value, Error> {
    let (name, target, goal, noise_budget): (String, Option<String>, String, f64) = conn
        .query_row(
            "SELECT name, target, goal, noise_budget FROM sessions WHERE id = ?1",
            params![session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    let count = |table: &str, extra: &str| -> Result<i64, Error> {
        let sql = format!("SELECT count(*) FROM {table} WHERE session_id = ?1{extra}");
        conn.query_row(&sql, params![session_id], |r| r.get(0))
            .map_err(|e| Error::Db(e.to_string()))
    };

    let hosts = count("hosts", "")?;
    let hyp_pending = count("hypotheses", " AND status = 'pending'")?;
    let hyp_confirmed = count("hypotheses", " AND status = 'confirmed'")?;
    let hyp_refuted = count("hypotheses", " AND status = 'refuted'")?;
    let hyp_total = hyp_pending + hyp_confirmed + hyp_refuted;

    let phase = derive_phase(hosts, hyp_total, hyp_pending, hyp_confirmed, hyp_refuted);

    Ok(serde_json::json!({
        "session_name": name,
        "target": target,
        "goal": goal,
        "phase": phase,
        "hosts": hosts,
        "ports": count("ports", "")?,
        "creds": count("credentials", "")?,
        "flags": count("flags", "")?,
        "access": count("access_levels", "")?,
        "hypotheses_pending": hyp_pending,
        "hypotheses_confirmed": hyp_confirmed,
        "hypotheses_refuted": hyp_refuted,
        "noise_budget": noise_budget,
    }))
}

pub fn derive_phase(
    hosts: i64,
    hyp_total: i64,
    hyp_pending: i64,
    hyp_confirmed: i64,
    hyp_refuted: i64,
) -> &'static str {
    if hosts == 0 && hyp_total == 0 {
        return "L0 — Setup";
    }
    if hosts > 0 && hyp_total == 0 {
        return "L1 — Surface Mapped";
    }
    if hyp_pending > 0 {
        return "L2 — Hypotheses Pending";
    }
    if hyp_confirmed > 0 && hyp_pending == 0 {
        return "L3 — Confirmed Available";
    }
    if hyp_pending == 0 && hyp_confirmed == 0 && hyp_refuted > 0 {
        return "L0 — Surface Exhausted";
    }
    "L0"
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        conn
    }

    #[test]
    fn test_list_sessions_filters_by_workspace() {
        let conn = setup_conn();
        create_session(&conn, "s1", "first", "/tmp/a", Some("10.0.0.1"), None, "general").unwrap();
        deactivate_session(&conn, "/tmp/a").unwrap();
        create_session(&conn, "s2", "second", "/tmp/a", Some("10.0.0.2"), None, "ctf").unwrap();
        create_session(&conn, "s3", "other", "/tmp/b", Some("10.0.0.3"), None, "general").unwrap();

        let rows = list_sessions(&conn, "/tmp/a").unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r["workspace_path"] == "/tmp/a"));
    }

    #[test]
    fn test_list_sessions_empty() {
        let conn = setup_conn();
        let rows = list_sessions(&conn, "/tmp/none").unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn test_list_all_sessions() {
        let conn = setup_conn();
        create_session(&conn, "s1", "first", "/tmp/a", None, None, "general").unwrap();
        create_session(&conn, "s2", "second", "/tmp/b", None, None, "general").unwrap();

        let rows = list_all_sessions(&conn).unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn test_find_session_by_name() {
        let conn = setup_conn();
        create_session(&conn, "s1", "mysess", "/tmp/a", None, None, "general").unwrap();

        let row = find_session_by_name_or_id(&conn, "mysess", "/tmp/a").unwrap();
        assert_eq!(row["id"], "s1");
    }

    #[test]
    fn test_find_session_by_id() {
        let conn = setup_conn();
        create_session(&conn, "s1", "mysess", "/tmp/a", None, None, "general").unwrap();

        let row = find_session_by_name_or_id(&conn, "s1", "/tmp/a").unwrap();
        assert_eq!(row["name"], "mysess");
    }

    #[test]
    fn test_find_session_wrong_workspace_fails() {
        let conn = setup_conn();
        create_session(&conn, "s1", "mysess", "/tmp/a", None, None, "general").unwrap();

        let result = find_session_by_name_or_id(&conn, "s1", "/tmp/b");
        assert!(result.is_err());
    }
}
