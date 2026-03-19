use rusqlite::Connection;
use crate::error::Error;

pub struct SkillMatch {
    pub skill_name: String,
    pub phase_label: String,
}

pub fn detect_phase(conn: &Connection, session_id: &str) -> Result<Option<SkillMatch>, Error> {
    let host_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM hosts WHERE session_id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    let hyp_pending: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM hypotheses WHERE session_id = ?1 AND status = 'pending'",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    let hyp_confirmed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM hypotheses WHERE session_id = ?1 AND status = 'confirmed'",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    let hyp_refuted: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM hypotheses WHERE session_id = ?1 AND status = 'refuted'",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    let hyp_total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM hypotheses WHERE session_id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    if host_count == 0 && hyp_total == 0 {
        return Ok(Some(SkillMatch {
            skill_name: "redtrail-recon".to_string(),
            phase_label: "Initial Recon".to_string(),
        }));
    }

    if host_count > 0 && hyp_total == 0 {
        return Ok(Some(SkillMatch {
            skill_name: "redtrail-hypothesize".to_string(),
            phase_label: "Surface Mapped".to_string(),
        }));
    }

    if hyp_pending > 0 {
        return Ok(Some(SkillMatch {
            skill_name: "redtrail-probe".to_string(),
            phase_label: "Hypotheses Pending".to_string(),
        }));
    }

    if hyp_confirmed > 0 && hyp_pending == 0 {
        return Ok(Some(SkillMatch {
            skill_name: "redtrail-exploit".to_string(),
            phase_label: "Confirmed Available".to_string(),
        }));
    }

    if hyp_pending == 0 && hyp_confirmed == 0 && hyp_refuted > 0 {
        return Ok(Some(SkillMatch {
            skill_name: "redtrail-recon".to_string(),
            phase_label: "Surface Exhausted".to_string(),
        }));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, name) VALUES ('s1', 'test')",
            [],
        ).unwrap();
        conn
    }

    #[test]
    fn test_detect_phase_empty_kb() {
        let conn = setup_db();
        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-recon");
    }

    #[test]
    fn test_detect_phase_surface_mapped() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.0.0.1')",
            [],
        ).unwrap();
        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-hypothesize");
    }

    #[test]
    fn test_detect_phase_hypotheses_pending() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.0.0.1')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO hypotheses (session_id, statement, category, status) VALUES ('s1', 'h1', 'auth', 'pending')",
            [],
        ).unwrap();
        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-probe");
    }

    #[test]
    fn test_detect_phase_confirmed_available() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.0.0.1')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO hypotheses (session_id, statement, category, status) VALUES ('s1', 'h1', 'auth', 'confirmed')",
            [],
        ).unwrap();
        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-exploit");
    }

    #[test]
    fn test_detect_phase_surface_exhausted() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.0.0.1')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO hypotheses (session_id, statement, category, status) VALUES ('s1', 'h1', 'auth', 'refuted')",
            [],
        ).unwrap();
        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-recon");
        assert_eq!(m.phase_label, "Surface Exhausted");
    }

    #[test]
    fn test_detect_phase_no_match() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.0.0.1')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO hypotheses (session_id, statement, category, status) VALUES ('s1', 'h1', 'auth', 'testing')",
            [],
        ).unwrap();
        let result = detect_phase(&conn, "s1").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_phase_returns_correct_skill_for_each_state() {
        let conn = setup_db();

        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-recon");

        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.0.0.1')",
            [],
        ).unwrap();
        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-hypothesize");

        conn.execute(
            "INSERT INTO hypotheses (session_id, statement, category, status) VALUES ('s1', 'h1', 'auth', 'pending')",
            [],
        ).unwrap();
        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-probe");

        conn.execute(
            "UPDATE hypotheses SET status = 'confirmed' WHERE session_id = 's1'",
            [],
        ).unwrap();
        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-exploit");

        conn.execute(
            "UPDATE hypotheses SET status = 'refuted' WHERE session_id = 's1'",
            [],
        ).unwrap();
        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-recon");
        assert_eq!(m.phase_label, "Surface Exhausted");
    }
}
