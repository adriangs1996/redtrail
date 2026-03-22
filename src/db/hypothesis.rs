use crate::error::Error;
use rusqlite::{Connection, params};

pub fn get(conn: &Connection, id: i64) -> Result<serde_json::Value, Error> {
    let mut hyp = conn.query_row(
        "SELECT id, statement, category, status, priority, confidence, target_component, created_at, resolved_at FROM hypotheses WHERE id = ?1",
        params![id],
        |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "statement": r.get::<_, String>(1)?,
                "category": r.get::<_, String>(2)?,
                "status": r.get::<_, String>(3)?,
                "priority": r.get::<_, String>(4)?,
                "confidence": r.get::<_, f64>(5)?,
                "target_component": r.get::<_, Option<String>>(6)?,
                "created_at": r.get::<_, String>(7)?,
                "resolved_at": r.get::<_, Option<String>>(8)?,
            }))
        },
    ).map_err(|e| Error::Db(e.to_string()))?;

    let mut stmt = conn.prepare(
        "SELECT id, finding, severity, poc, created_at FROM evidence WHERE hypothesis_id = ?1 ORDER BY created_at DESC"
    ).map_err(|e| Error::Db(e.to_string()))?;
    let evidence: Vec<serde_json::Value> = stmt
        .query_map(params![id], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "finding": r.get::<_, String>(1)?,
                "severity": r.get::<_, String>(2)?,
                "poc": r.get::<_, Option<String>>(3)?,
                "created_at": r.get::<_, String>(4)?,
            }))
        })
        .map_err(|e| Error::Db(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Db(e.to_string()))?;

    hyp["evidence"] = serde_json::Value::Array(evidence);
    Ok(hyp)
}

fn map_hypothesis_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "id": r.get::<_, i64>(0)?,
        "statement": r.get::<_, String>(1)?,
        "category": r.get::<_, String>(2)?,
        "status": r.get::<_, String>(3)?,
        "priority": r.get::<_, String>(4)?,
        "confidence": r.get::<_, f64>(5)?,
        "target_component": r.get::<_, Option<String>>(6)?,
        "created_at": r.get::<_, String>(7)?,
    }))
}

fn map_evidence_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "id": r.get::<_, i64>(0)?,
        "hypothesis_id": r.get::<_, Option<i64>>(1)?,
        "finding": r.get::<_, String>(2)?,
        "severity": r.get::<_, String>(3)?,
        "poc": r.get::<_, Option<String>>(4)?,
        "created_at": r.get::<_, String>(5)?,
    }))
}

pub fn list(
    conn: &Connection,
    session_id: &str,
    status_filter: Option<&str>,
) -> Result<Vec<serde_json::Value>, Error> {
    if let Some(status) = status_filter {
        let mut stmt = conn.prepare(
            "SELECT id, statement, category, status, priority, confidence, target_component, created_at FROM hypotheses WHERE session_id = ?1 AND status = ?2 ORDER BY created_at DESC"
        ).map_err(|e| Error::Db(e.to_string()))?;
        return stmt
            .query_map(params![session_id, status], map_hypothesis_row)
            .map_err(|e| Error::Db(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Error::Db(e.to_string()));
    }
    let mut stmt = conn.prepare(
        "SELECT id, statement, category, status, priority, confidence, target_component, created_at FROM hypotheses WHERE session_id = ?1 ORDER BY created_at DESC"
    ).map_err(|e| Error::Db(e.to_string()))?;
    stmt.query_map(params![session_id], map_hypothesis_row)
        .map_err(|e| Error::Db(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Db(e.to_string()))
}

pub fn list_evidence(
    conn: &Connection,
    session_id: &str,
    hypothesis_id: Option<i64>,
) -> Result<Vec<serde_json::Value>, Error> {
    if let Some(hid) = hypothesis_id {
        let mut stmt = conn.prepare(
            "SELECT id, hypothesis_id, finding, severity, poc, created_at FROM evidence WHERE session_id = ?1 AND hypothesis_id = ?2 ORDER BY created_at DESC"
        ).map_err(|e| Error::Db(e.to_string()))?;
        return stmt
            .query_map(params![session_id, hid], map_evidence_row)
            .map_err(|e| Error::Db(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Error::Db(e.to_string()));
    }
    let mut stmt = conn.prepare(
        "SELECT id, hypothesis_id, finding, severity, poc, created_at FROM evidence WHERE session_id = ?1 ORDER BY created_at DESC"
    ).map_err(|e| Error::Db(e.to_string()))?;
    stmt.query_map(params![session_id], map_evidence_row)
        .map_err(|e| Error::Db(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Db(e.to_string()))
}

pub fn export_evidence(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<serde_json::Value>, Error> {
    let mut hyp_stmt = conn
        .prepare("SELECT id, statement, category, status FROM hypotheses WHERE session_id = ?1")
        .map_err(|e| Error::Db(e.to_string()))?;
    let hypotheses: Vec<(i64, String, String, String)> = hyp_stmt
        .query_map(params![session_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(|e| Error::Db(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Db(e.to_string()))?;

    let mut result = Vec::new();

    for (hid, statement, category, status) in hypotheses {
        let mut ev_stmt = conn.prepare(
            "SELECT id, finding, severity, poc, created_at FROM evidence WHERE hypothesis_id = ?1"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let evidence: Vec<serde_json::Value> = ev_stmt
            .query_map(params![hid], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?,
                    "finding": r.get::<_, String>(1)?,
                    "severity": r.get::<_, String>(2)?,
                    "poc": r.get::<_, Option<String>>(3)?,
                    "created_at": r.get::<_, String>(4)?,
                }))
            })
            .map_err(|e| Error::Db(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Error::Db(e.to_string()))?;

        result.push(serde_json::json!({
            "hypothesis_id": hid,
            "statement": statement,
            "category": category,
            "status": status,
            "evidence": evidence,
        }));
    }

    let mut orphan_stmt = conn.prepare(
        "SELECT id, finding, severity, poc, created_at FROM evidence WHERE session_id = ?1 AND hypothesis_id IS NULL"
    ).map_err(|e| Error::Db(e.to_string()))?;
    let orphans: Vec<serde_json::Value> = orphan_stmt
        .query_map(params![session_id], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "finding": r.get::<_, String>(1)?,
                "severity": r.get::<_, String>(2)?,
                "poc": r.get::<_, Option<String>>(3)?,
                "created_at": r.get::<_, String>(4)?,
            }))
        })
        .map_err(|e| Error::Db(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::Db(e.to_string()))?;

    if !orphans.is_empty() {
        result.push(serde_json::json!({
            "hypothesis_id": null,
            "statement": null,
            "category": null,
            "status": null,
            "evidence": orphans,
        }));
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::db::{Hypotheses, SessionOps, open_in_memory};

    #[test]
    fn test_list_hypotheses_empty() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", "/tmp/test", None, None, "general").unwrap();
        let rows = db.list_hypotheses("s1", None).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn test_list_evidence_empty() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", "/tmp/test", None, None, "general").unwrap();
        let rows = db.list_evidence("s1", None).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn test_export_evidence_empty() {
        let db = open_in_memory().unwrap();
        db.create_session("s1", "test", "/tmp/test", None, None, "general").unwrap();
        let rows = db.export_evidence("s1").unwrap();
        assert!(rows.is_empty());
    }
}
