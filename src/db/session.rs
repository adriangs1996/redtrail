use rusqlite::{Connection, params};
use crate::error::Error;

pub fn active_session_id(conn: &Connection) -> Result<String, Error> {
    conn.query_row(
        "SELECT id FROM sessions LIMIT 1", [], |r| r.get(0),
    ).map_err(|_| Error::NoActiveSession)
}

pub fn load_flag_patterns(conn: &Connection, session_id: &str) -> Result<Vec<String>, Error> {
    let meta: Option<String> = conn.query_row(
        "SELECT goal_meta FROM sessions WHERE id = ?1",
        params![session_id],
        |r| r.get(0),
    ).map_err(|e| Error::Db(e.to_string()))?;

    if let Some(m) = meta
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&m)
            && let Some(arr) = v.get("flag_patterns").and_then(|x| x.as_array()) {
                let pats: Vec<String> = arr.iter()
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
    let scope: Option<String> = conn.query_row(
        "SELECT scope FROM sessions WHERE id = ?1",
        params![session_id],
        |r| r.get(0),
    ).map_err(|e| Error::Db(e.to_string()))?;
    Ok(scope.filter(|s| !s.is_empty()))
}

pub fn decrement_noise_budget(conn: &Connection, session_id: &str, cost: f64) -> Result<(), Error> {
    conn.execute(
        "UPDATE sessions SET noise_budget = max(0, noise_budget - ?1) WHERE id = ?2",
        params![cost, session_id],
    ).map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn create_session(conn: &Connection, id: &str, name: &str, target: Option<&str>, scope: Option<&str>, goal: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO sessions (id, name, target, scope, goal) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, name, target, scope, goal],
    ).map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn get_session(conn: &Connection, session_id: &str) -> Result<serde_json::Value, Error> {
    conn.query_row(
        "SELECT id, name, target, scope, goal, phase, autonomy, noise_budget, created_at, updated_at FROM sessions WHERE id = ?1",
        params![session_id],
        |r| Ok(serde_json::json!({
            "id": r.get::<_, String>(0)?,
            "name": r.get::<_, String>(1)?,
            "target": r.get::<_, Option<String>>(2)?,
            "scope": r.get::<_, Option<String>>(3)?,
            "goal": r.get::<_, String>(4)?,
            "phase": r.get::<_, String>(5)?,
            "autonomy": r.get::<_, String>(6)?,
            "noise_budget": r.get::<_, f64>(7)?,
            "created_at": r.get::<_, String>(8)?,
            "updated_at": r.get::<_, String>(9)?,
        })),
    ).map_err(|e| Error::Db(e.to_string()))
}

pub fn status_summary(conn: &Connection, session_id: &str) -> Result<serde_json::Value, Error> {
    let (name, target, goal, phase, noise_budget): (String, Option<String>, String, String, f64) = conn.query_row(
        "SELECT name, target, goal, phase, noise_budget FROM sessions WHERE id = ?1",
        params![session_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    ).map_err(|e| Error::Db(e.to_string()))?;

    let count = |table: &str, extra: &str| -> Result<i64, Error> {
        let sql = format!("SELECT count(*) FROM {table} WHERE session_id = ?1{extra}");
        conn.query_row(&sql, params![session_id], |r| r.get(0))
            .map_err(|e| Error::Db(e.to_string()))
    };

    Ok(serde_json::json!({
        "session_name": name,
        "target": target,
        "goal": goal,
        "phase": phase,
        "hosts": count("hosts", "")?,
        "ports": count("ports", "")?,
        "creds": count("credentials", "")?,
        "flags": count("flags", "")?,
        "access": count("access_levels", "")?,
        "hypotheses_pending": count("hypotheses", " AND status = 'pending'")?,
        "hypotheses_confirmed": count("hypotheses", " AND status = 'confirmed'")?,
        "hypotheses_refuted": count("hypotheses", " AND status = 'refuted'")?,
        "noise_budget": noise_budget,
    }))
}
