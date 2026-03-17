use rusqlite::params;
use crate::db_v2::DbV2;
use crate::error::Error;
use super::SessionContext;

pub fn save(db: &DbV2, ctx: &SessionContext) -> Result<(), Error> {
    let env_json = serde_json::to_string(&ctx.env)
        .map_err(|e| Error::Db(e.to_string()))?;
    let tool_json = serde_json::to_string(&ctx.tool_config)
        .map_err(|e| Error::Db(e.to_string()))?;

    db.conn().execute(
        "INSERT INTO sessions
            (id, name, created_at, updated_at, env_json, tool_config_json,
             llm_provider, llm_model, working_dir, prompt_template)
         VALUES (?1, ?2, ?3, datetime('now'), ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
             name = excluded.name,
             updated_at = datetime('now'),
             env_json = excluded.env_json,
             tool_config_json = excluded.tool_config_json,
             llm_provider = excluded.llm_provider,
             llm_model = excluded.llm_model,
             working_dir = excluded.working_dir,
             prompt_template = excluded.prompt_template",
        params![
            ctx.id, ctx.name, ctx.created_at,
            env_json, tool_json,
            ctx.llm_provider, ctx.llm_model,
            ctx.working_dir.to_string_lossy().to_string(),
            ctx.prompt_template,
        ],
    ).map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn load(db: &DbV2, id: &str) -> Result<SessionContext, Error> {
    let conn = db.conn();
    conn.query_row(
        "SELECT id, name, created_at, env_json, tool_config_json,
                llm_provider, llm_model, working_dir, prompt_template
         FROM sessions WHERE id = ?1",
        params![id],
        |row| {
            let env_str: String = row.get(3)?;
            let tool_str: String = row.get(4)?;
            let wd_str: String = row.get(7)?;

            Ok(SessionContext {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                env: serde_json::from_str(&env_str).unwrap_or_default(),
                target: crate::types::Target {
                    base_url: None,
                    hosts: vec![],
                    exec_mode: crate::types::ExecMode::Local,
                    auth_token: None,
                    scope: vec![],
                },
                tool_config: serde_json::from_str(&tool_str).unwrap_or_default(),
                llm_provider: row.get(5)?,
                llm_model: row.get(6)?,
                working_dir: std::path::PathBuf::from(wd_str),
                prompt_template: row.get(8)?,
            })
        },
    ).map_err(|e| Error::Db(e.to_string()))
}

pub fn list(db: &DbV2) -> Result<Vec<SessionContext>, Error> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT id FROM sessions ORDER BY created_at DESC"
    ).map_err(|e| Error::Db(e.to_string()))?;

    let ids: Vec<String> = stmt.query_map([], |row| row.get(0))
        .map_err(|e| Error::Db(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    ids.iter().map(|id| load(db, id)).collect()
}

pub fn delete(db: &DbV2, id: &str) -> Result<(), Error> {
    let conn = db.conn();
    conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])
        .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

pub fn load_by_name(db: &DbV2, name: &str) -> Result<SessionContext, Error> {
    let id: String = db.conn().query_row(
        "SELECT id FROM sessions WHERE name = ?1",
        params![name],
        |row| row.get(0),
    ).map_err(|e| Error::Db(e.to_string()))?;
    load(db, &id)
}

pub fn clone_session(db: &DbV2, src_id: &str, dest_name: &str) -> Result<SessionContext, Error> {
    let src = load(db, src_id)?;
    let mut dest = src.clone();
    dest.id = uuid::Uuid::new_v4().to_string();
    dest.name = dest_name.to_string();
    dest.created_at = chrono::Utc::now().to_rfc3339();
    save(db, &dest)?;

    let conn = db.conn();
    let mut host_map = std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, ip, hostname, os, status FROM hosts WHERE session_id = ?1"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows: Vec<(i64, String, Option<String>, Option<String>, String)> = stmt.query_map(
            params![src_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        ).map_err(|e| Error::Db(e.to_string()))?
        .filter_map(|r| r.ok()).collect();

        for (old_id, ip, hostname, os, status) in rows {
            conn.execute(
                "INSERT INTO hosts (session_id, ip, hostname, os, status) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![dest.id, ip, hostname, os, status],
            ).map_err(|e| Error::Db(e.to_string()))?;
            host_map.insert(old_id, conn.last_insert_rowid());
        }
    }

    {
        let mut stmt = conn.prepare(
            "SELECT host_id, port, protocol, service, version FROM ports WHERE session_id = ?1"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows: Vec<(i64, i32, String, Option<String>, Option<String>)> = stmt.query_map(
            params![src_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        ).map_err(|e| Error::Db(e.to_string()))?
        .filter_map(|r| r.ok()).collect();

        for (old_hid, port, proto, svc, ver) in rows {
            let new_hid = host_map.get(&old_hid).copied().unwrap_or(old_hid);
            conn.execute(
                "INSERT INTO ports (session_id, host_id, port, protocol, service, version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![dest.id, new_hid, port, proto, svc, ver],
            ).map_err(|e| Error::Db(e.to_string()))?;
        }
    }

    {
        let mut stmt = conn.prepare(
            "SELECT username, password, hash, source, host_id FROM credentials WHERE session_id = ?1"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows: Vec<(String, Option<String>, Option<String>, Option<String>, Option<i64>)> = stmt.query_map(
            params![src_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        ).map_err(|e| Error::Db(e.to_string()))?
        .filter_map(|r| r.ok()).collect();

        for (user, pass, hash, source, old_hid) in rows {
            let new_hid = old_hid.and_then(|h| host_map.get(&h).copied());
            conn.execute(
                "INSERT INTO credentials (session_id, username, password, hash, source, host_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![dest.id, user, pass, hash, source, new_hid],
            ).map_err(|e| Error::Db(e.to_string()))?;
        }
    }

    {
        let mut stmt = conn.prepare(
            "SELECT host_id, user, level, method FROM access_levels WHERE session_id = ?1"
        ).map_err(|e| Error::Db(e.to_string()))?;
        let rows: Vec<(i64, String, String, Option<String>)> = stmt.query_map(
            params![src_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).map_err(|e| Error::Db(e.to_string()))?
        .filter_map(|r| r.ok()).collect();

        for (old_hid, user, level, method) in rows {
            let new_hid = host_map.get(&old_hid).copied().unwrap_or(old_hid);
            conn.execute(
                "INSERT INTO access_levels (session_id, host_id, user, level, method)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![dest.id, new_hid, user, level, method],
            ).map_err(|e| Error::Db(e.to_string()))?;
        }
    }

    for (table, cols) in &[
        ("flags", "session_id, value, source, captured_at"),
        ("attack_paths", "session_id, from_host, to_host, technique, status"),
        ("findings", "session_id, type, severity, title, description, evidence"),
        ("command_history", "session_id, command, exit_code, started_at, duration_ms, output_preview, block_id"),
        ("failed_attempts", "session_id, technique, target, reason, timestamp"),
    ] {
        let non_session_cols = cols.replacen("session_id, ", "", 1);
        let sql = format!(
            "INSERT INTO {table} ({cols}) SELECT ?1, {non_session_cols} FROM {table} WHERE session_id = ?2"
        );
        conn.execute(&sql, params![dest.id, src_id])
            .map_err(|e| Error::Db(e.to_string()))?;
    }

    Ok(dest)
}
