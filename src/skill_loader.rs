use crate::error::Error;
use rusqlite::Connection;
use std::path::Path;

pub const KNOWN_TOOL_NAMES: &[&str] = &[
    "query_table",
    "create_record",
    "update_record",
    "run_command",
    "suggest",
    "respond",
];

pub struct SkillMatch {
    pub phase_name: String,
    pub skill_name: String,
    pub context: String,
}

pub struct SkillConfig {
    pub tools: Option<Vec<String>>,
}

pub fn detect_phase(conn: &Connection, session_id: &str) -> Result<Option<SkillMatch>, Error> {
    let (host_count, hyp_total, hyp_pending, hyp_confirmed, hyp_refuted): (
        i64,
        i64,
        i64,
        i64,
        i64,
    ) = conn
        .query_row(
            "SELECT
            (SELECT count(*) FROM hosts WHERE session_id = ?1),
            (SELECT count(*) FROM hypotheses WHERE session_id = ?1),
            (SELECT count(*) FROM hypotheses WHERE session_id = ?1 AND status = 'pending'),
            (SELECT count(*) FROM hypotheses WHERE session_id = ?1 AND status = 'confirmed'),
            (SELECT count(*) FROM hypotheses WHERE session_id = ?1 AND status = 'refuted')",
            rusqlite::params![session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    phase_from_counts(
        host_count,
        hyp_total,
        hyp_pending,
        hyp_confirmed,
        hyp_refuted,
    )
}

fn phase_from_counts(
    hosts: i64,
    hyp_total: i64,
    hyp_pending: i64,
    hyp_confirmed: i64,
    hyp_refuted: i64,
) -> Result<Option<SkillMatch>, Error> {
    if hosts == 0 && hyp_total == 0 {
        return Ok(Some(SkillMatch {
            phase_name: "Setup".into(),
            skill_name: "redtrail-recon".into(),
            context: "no hosts discovered".into(),
        }));
    }
    if hosts > 0 && hyp_total == 0 {
        return Ok(Some(SkillMatch {
            phase_name: "Surface Mapped".into(),
            skill_name: "redtrail-hypothesize".into(),
            context: format!("{hosts} hosts, no hypotheses"),
        }));
    }
    if hyp_pending > 0 {
        return Ok(Some(SkillMatch {
            phase_name: "Hypotheses Pending".into(),
            skill_name: "redtrail-probe".into(),
            context: format!("{hyp_pending} pending"),
        }));
    }
    if hyp_confirmed > 0 && hyp_pending == 0 {
        return Ok(Some(SkillMatch {
            phase_name: "Confirmed Available".into(),
            skill_name: "redtrail-exploit".into(),
            context: format!("{hyp_confirmed} confirmed"),
        }));
    }
    if hyp_pending == 0 && hyp_confirmed == 0 && hyp_refuted > 0 {
        return Ok(Some(SkillMatch {
            phase_name: "Surface Exhausted".into(),
            skill_name: "redtrail-recon".into(),
            context: format!("all {hyp_refuted} refuted, widening"),
        }));
    }
    Ok(None)
}

fn bundled_prompt(skill_name: &str) -> Option<&'static str> {
    match skill_name {
        "redtrail-recon" => Some(include_str!("../skills/redtrail-recon/prompt.md")),
        "redtrail-hypothesize" => Some(include_str!("../skills/redtrail-hypothesize/prompt.md")),
        "redtrail-probe" => Some(include_str!("../skills/redtrail-probe/prompt.md")),
        "redtrail-exploit" => Some(include_str!("../skills/redtrail-exploit/prompt.md")),
        "redtrail-report" => Some(include_str!("../skills/redtrail-report/prompt.md")),
        _ => None,
    }
}

pub fn load_skill_prompt(skill_name: &str, workspace: Option<&Path>) -> Result<String, Error> {
    if let Some(home) = dirs::home_dir() {
        let installed = home
            .join(".redtrail/skills")
            .join(skill_name)
            .join("prompt.md");
        if installed.exists() {
            return std::fs::read_to_string(&installed).map_err(Error::Io);
        }
    }
    if let Some(ws) = workspace {
        let ws_skill = ws.join("skills").join(skill_name).join("prompt.md");
        if ws_skill.exists() {
            return std::fs::read_to_string(&ws_skill).map_err(Error::Io);
        }
    }
    if let Some(prompt) = bundled_prompt(skill_name) {
        return Ok(prompt.to_string());
    }
    Err(Error::SkillNotFound(skill_name.to_string()))
}

fn bundled_toml(skill_name: &str) -> Option<&'static str> {
    match skill_name {
        "redtrail-recon" => Some(include_str!("../skills/redtrail-recon/skill.toml")),
        "redtrail-hypothesize" => Some(include_str!("../skills/redtrail-hypothesize/skill.toml")),
        "redtrail-probe" => Some(include_str!("../skills/redtrail-probe/skill.toml")),
        "redtrail-exploit" => Some(include_str!("../skills/redtrail-exploit/skill.toml")),
        "redtrail-report" => Some(include_str!("../skills/redtrail-report/skill.toml")),
        _ => None,
    }
}

fn parse_tools_from_toml(content: &str) -> Option<Vec<String>> {
    let val: toml::Value = toml::from_str(content).ok()?;
    let arr = val.get("tools")?.as_array()?;
    Some(arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
}

pub fn load_skill_config(skill_name: &str, workspace: Option<&Path>) -> SkillConfig {
    let toml_content = if let Some(home) = dirs::home_dir() {
        let installed = home
            .join(".redtrail/skills")
            .join(skill_name)
            .join("skill.toml");
        if installed.exists() {
            std::fs::read_to_string(&installed).ok()
        } else {
            None
        }
    } else {
        None
    }
    .or_else(|| {
        workspace.and_then(|ws| {
            let ws_skill = ws.join("skills").join(skill_name).join("skill.toml");
            if ws_skill.exists() {
                std::fs::read_to_string(&ws_skill).ok()
            } else {
                None
            }
        })
    })
    .or_else(|| bundled_toml(skill_name).map(String::from));

    let tools = toml_content.and_then(|c| parse_tools_from_toml(&c));
    SkillConfig { tools }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_load_skill_prompt_from_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills/redtrail-recon");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("prompt.md"), "# Recon skill prompt").unwrap();
        let result = load_skill_prompt("redtrail-recon", Some(tmp.path())).unwrap();
        assert_eq!(result, "# Recon skill prompt");
    }

    #[test]
    fn test_load_skill_prompt_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let result = load_skill_prompt("nonexistent-skill", Some(tmp.path()));
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("nonexistent-skill"));
    }

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        conn.execute("INSERT INTO sessions (id, name) VALUES ('s1', 'test')", [])
            .unwrap();
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
        )
        .unwrap();
        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-hypothesize");
    }

    #[test]
    fn test_detect_phase_hypotheses_pending() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.0.0.1')",
            [],
        )
        .unwrap();
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
        )
        .unwrap();
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
        )
        .unwrap();
        conn.execute(
            "INSERT INTO hypotheses (session_id, statement, category, status) VALUES ('s1', 'h1', 'auth', 'refuted')",
            [],
        ).unwrap();
        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-recon");
        assert_eq!(m.phase_name, "Surface Exhausted");
    }

    #[test]
    fn test_detect_phase_no_match() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.0.0.1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO hypotheses (session_id, statement, category, status) VALUES ('s1', 'h1', 'auth', 'testing')",
            [],
        ).unwrap();
        let result = detect_phase(&conn, "s1").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_build_prompt_with_skill_replaces_identity() {
        let conn = setup_db();
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills/redtrail-recon");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("prompt.md"),
            "# Recon skill\nYou are the recon advisor.",
        )
        .unwrap();

        let prompt =
            crate::agent::assistant::build_system_prompt(&conn, "s1", tmp.path(), None, false).unwrap();

        assert!(
            prompt.contains("Recon skill"),
            "should contain skill content"
        );
        assert!(
            prompt.contains("Active skill: redtrail-recon"),
            "should have skill header"
        );
        assert!(
            !prompt.contains("You are Redtrail, a pentesting advisor"),
            "should NOT contain generic identity"
        );
    }

    #[test]
    fn test_build_prompt_no_skill_uses_generic() {
        let conn = setup_db();
        let tmp = tempfile::tempdir().unwrap();

        let prompt =
            crate::agent::assistant::build_system_prompt(&conn, "s1", tmp.path(), None, true).unwrap();

        assert!(
            prompt.contains("You are Redtrail, a pentesting advisor"),
            "should contain generic identity"
        );
        assert!(
            !prompt.contains("Active skill:"),
            "should NOT have skill header"
        );
    }

    #[test]
    fn test_build_prompt_skill_override() {
        let conn = setup_db();
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills/redtrail-hypothesize");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("prompt.md"),
            "# Hypothesize\nGenerate hypotheses.",
        )
        .unwrap();

        let prompt = crate::agent::assistant::build_system_prompt(
            &conn,
            "s1",
            tmp.path(),
            Some("redtrail-hypothesize"),
            false,
        )
        .unwrap();

        assert!(
            prompt.contains("Active skill: redtrail-hypothesize"),
            "should load overridden skill"
        );
        assert!(
            prompt.contains("Generate hypotheses"),
            "should contain override skill content"
        );
    }

    #[test]
    fn test_build_prompt_uses_bundled_skill_without_workspace_files() {
        let conn = setup_db();
        let tmp = tempfile::tempdir().unwrap();

        let prompt =
            crate::agent::assistant::build_system_prompt(&conn, "s1", tmp.path(), None, false).unwrap();

        assert!(
            prompt.contains("Active skill: redtrail-recon"),
            "should load bundled skill"
        );
        assert!(
            prompt.contains("L0 Modeling"),
            "should contain bundled recon content"
        );
        assert!(
            !prompt.contains("You are Redtrail, a pentesting advisor"),
            "should NOT use generic"
        );
    }

    #[test]
    fn test_load_skill_prompt_bundled_fallback() {
        let result = load_skill_prompt("redtrail-recon", None).unwrap();
        assert!(
            result.contains("L0 Modeling"),
            "bundled recon skill should contain L0 Modeling"
        );
    }

    #[test]
    fn test_load_skill_prompt_workspace_overrides_bundled() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills/redtrail-recon");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("prompt.md"), "# Custom recon override").unwrap();

        let result = load_skill_prompt("redtrail-recon", Some(tmp.path())).unwrap();
        assert_eq!(
            result, "# Custom recon override",
            "workspace should override bundled"
        );
    }

    #[test]
    fn test_build_prompt_kb_dump_follows_skill() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.10.10.1')",
            [],
        )
        .unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills/redtrail-hypothesize");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("prompt.md"), "# Hypothesize").unwrap();

        let prompt =
            crate::agent::assistant::build_system_prompt(&conn, "s1", tmp.path(), None, false).unwrap();

        assert!(prompt.contains("=== Hosts ==="), "should contain KB dump");
        assert!(prompt.contains("10.10.10.1"), "should contain host data");
        let skill_pos = prompt.find("Hypothesize").unwrap();
        let kb_pos = prompt.find("=== Hosts ===").unwrap();
        assert!(skill_pos < kb_pos, "skill content should precede KB dump");
    }

    #[test]
    fn test_detect_phase_returns_correct_skill_for_each_state() {
        let conn = setup_db();

        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-recon");

        conn.execute(
            "INSERT INTO hosts (session_id, ip) VALUES ('s1', '10.0.0.1')",
            [],
        )
        .unwrap();
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
        )
        .unwrap();
        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-exploit");

        conn.execute(
            "UPDATE hypotheses SET status = 'refuted' WHERE session_id = 's1'",
            [],
        )
        .unwrap();
        let m = detect_phase(&conn, "s1").unwrap().unwrap();
        assert_eq!(m.skill_name, "redtrail-recon");
        assert_eq!(m.phase_name, "Surface Exhausted");
    }

    #[test]
    fn test_parse_tools_from_toml_present() {
        let toml = r#"
name = "test"
tools = ["query_table", "suggest"]
"#;
        let tools = parse_tools_from_toml(toml).unwrap();
        assert_eq!(tools, vec!["query_table", "suggest"]);
    }

    #[test]
    fn test_parse_tools_from_toml_missing() {
        let toml = r#"
name = "test"
"#;
        assert!(parse_tools_from_toml(toml).is_none());
    }

    #[test]
    fn test_parse_tools_from_toml_empty() {
        let toml = r#"
name = "test"
tools = []
"#;
        let tools = parse_tools_from_toml(toml).unwrap();
        assert!(tools.is_empty());
    }

    #[test]
    fn test_load_skill_config_bundled_recon_has_tools() {
        let cfg = load_skill_config("redtrail-recon", None);
        let tools = cfg.tools.unwrap();
        assert!(tools.contains(&"query_table".to_string()));
        assert!(tools.contains(&"run_command".to_string()));
    }

    #[test]
    fn test_load_skill_config_bundled_report_restricted() {
        let cfg = load_skill_config("redtrail-report", None);
        let tools = cfg.tools.unwrap();
        assert!(tools.contains(&"query_table".to_string()));
        assert!(!tools.contains(&"run_command".to_string()));
    }

    #[test]
    fn test_load_skill_config_unknown_skill_no_tools() {
        let cfg = load_skill_config("nonexistent", None);
        assert!(cfg.tools.is_none());
    }

    #[test]
    fn test_load_skill_config_workspace_override() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills/redtrail-recon");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("skill.toml"),
            "name = \"redtrail-recon\"\ntools = [\"suggest\"]\n",
        )
        .unwrap();
        let cfg = load_skill_config("redtrail-recon", Some(tmp.path()));
        let tools = cfg.tools.unwrap();
        assert_eq!(tools, vec!["suggest"]);
    }
}
