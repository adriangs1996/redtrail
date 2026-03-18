use crate::db::Db;
use crate::error::Error;

pub fn extract_sync(db: &Db, session_id: &str, cmd_id: i64, _config: &crate::config::Config) -> Result<(), Error> {
    let (row_session_id, command, tool, output) = db.get_command_for_extraction(cmd_id)?;
    let _ = row_session_id;

    let output = match output {
        None => {
            db.update_extraction_status(cmd_id, "skipped")?;
            return Ok(());
        }
        Some(o) if o.trim().is_empty() => {
            db.update_extraction_status(cmd_id, "skipped")?;
            return Ok(());
        }
        Some(o) => o,
    };

    let truncated = if output.len() > 8000 {
        let mut end = 8000;
        while end > 0 && !output.is_char_boundary(end) { end -= 1; }
        &output[..end]
    } else {
        &output
    };
    let tool_str = tool.as_deref().unwrap_or("unknown");

    let prompt = format!(
        "You are a pentesting data extractor. Given command output, extract structured data.\n\nCommand: {command}\nTool: {tool_str}\n\nOutput:\n{truncated}\n\nReturn ONLY valid JSON:\n{{\"hosts\":[{{\"ip\":\"...\",\"hostname\":\"...\",\"os\":\"...\"}}],\"ports\":[{{\"ip\":\"...\",\"port\":22,\"protocol\":\"tcp\",\"service\":\"ssh\",\"version\":\"...\"}}],\"credentials\":[{{\"username\":\"...\",\"password\":\"...\",\"service\":\"...\",\"host\":\"...\"}}],\"flags\":[{{\"value\":\"...\",\"source\":\"...\"}}],\"access\":[{{\"host\":\"...\",\"user\":\"...\",\"level\":\"...\",\"method\":\"...\"}}],\"notes\":[\"...\"]}}\n\nEmpty arrays for categories with no data found."
    );

    let config = _config;
    match call_llm(&prompt, config) {
        Ok(text) => {
            let json_str = extract_json(&text);
            match apply_extraction(db, session_id, json_str) {
                Ok(()) => db.update_extraction_status(cmd_id, "done")?,
                Err(_) => db.update_extraction_status(cmd_id, "failed")?,
            }
        }
        Err(_) => {
            db.update_extraction_status(cmd_id, "failed")?;
            return Err(Error::Config("LLM extraction failed".into()));
        }
    }

    Ok(())
}

pub fn apply_extraction(db: &Db, session_id: &str, json_str: &str) -> Result<(), Error> {
    let v: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| Error::Config(format!("invalid JSON from LLM: {e}")))?;

    if let Some(hosts) = v["hosts"].as_array() {
        for h in hosts {
            let ip = match h["ip"].as_str() { Some(s) if !s.is_empty() && s != "..." => s, _ => continue };
            let os = h["os"].as_str().filter(|s| !s.is_empty() && *s != "...");
            let hostname = h["hostname"].as_str().filter(|s| !s.is_empty() && *s != "...");
            db.add_host(session_id, ip, os, hostname)?;
        }
    }

    if let Some(ports) = v["ports"].as_array() {
        for p in ports {
            let ip = match p["ip"].as_str() { Some(s) if !s.is_empty() && s != "..." => s, _ => continue };
            let port = match p["port"].as_i64() { Some(n) if n > 0 => n, _ => continue };
            let protocol = p["protocol"].as_str().filter(|s| !s.is_empty() && *s != "...");
            let service = p["service"].as_str().filter(|s| !s.is_empty() && *s != "...");
            let version = p["version"].as_str().filter(|s| !s.is_empty() && *s != "...");
            db.add_port(session_id, ip, port, protocol, service, version)?;
        }
    }

    if let Some(creds) = v["credentials"].as_array() {
        for c in creds {
            let username = match c["username"].as_str() { Some(s) if !s.is_empty() && s != "..." => s, _ => continue };
            let password = c["password"].as_str().filter(|s| !s.is_empty() && *s != "...");
            let service = c["service"].as_str().filter(|s| !s.is_empty() && *s != "...");
            let host = c["host"].as_str().filter(|s| !s.is_empty() && *s != "...");
            db.add_credential(session_id, username, password, None, service, host, Some("llm-extraction"))?;
        }
    }

    if let Some(flags) = v["flags"].as_array() {
        for f in flags {
            let value = match f["value"].as_str() { Some(s) if !s.is_empty() && s != "..." => s, _ => continue };
            let source = f["source"].as_str().filter(|s| !s.is_empty() && *s != "...");
            db.add_flag(session_id, value, source)?;
        }
    }

    if let Some(access) = v["access"].as_array() {
        for a in access {
            let host = match a["host"].as_str() { Some(s) if !s.is_empty() && s != "..." => s, _ => continue };
            let user = match a["user"].as_str() { Some(s) if !s.is_empty() && s != "..." => s, _ => continue };
            let level = match a["level"].as_str() { Some(s) if !s.is_empty() && s != "..." => s, _ => continue };
            let method = a["method"].as_str().filter(|s| !s.is_empty() && *s != "...");
            db.add_access(session_id, host, user, level, method)?;
        }
    }

    if let Some(notes) = v["notes"].as_array() {
        for n in notes {
            let text = match n.as_str() { Some(s) if !s.is_empty() && s != "..." => s, _ => continue };
            db.add_note(session_id, text)?;
        }
    }

    Ok(())
}

fn call_llm(prompt: &str, config: &crate::config::Config) -> Result<String, Error> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| Error::Config("ANTHROPIC_API_KEY not set".into()))?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| Error::Config(e.to_string()))?;
    let resp = client.post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": config.general.llm_model,
            "max_tokens": 4096,
            "messages": [{"role": "user", "content": prompt}]
        }))
        .send()
        .map_err(|e| Error::Config(format!("LLM request: {e}")))?;
    let body: serde_json::Value = resp.json()
        .map_err(|e| Error::Config(format!("LLM response: {e}")))?;
    body["content"][0]["text"].as_str()
        .map(String::from)
        .ok_or(Error::Config("no text in LLM response".into()))
}

fn extract_json(text: &str) -> &str {
    if let Some(start) = text.find('{')
        && let Some(end) = text.rfind('}') {
            return &text[start..=end];
        }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn test_extract_json_plain() {
        assert_eq!(extract_json(r#"{"hosts":[]}"#), r#"{"hosts":[]}"#);
    }

    #[test]
    fn test_extract_json_markdown_fences() {
        let input = "Here:\n```json\n{\"hosts\":[]}\n```\nDone";
        assert_eq!(extract_json(input), "{\"hosts\":[]}");
    }

    #[test]
    fn test_apply_extraction_hosts_and_ports() {
        let db = Db::open_in_memory().unwrap();
        db.conn().execute("INSERT INTO sessions (id, name) VALUES ('s1', 'test')", []).unwrap();

        let json = r#"{"hosts":[{"ip":"10.10.10.1","os":"Linux"}],"ports":[{"ip":"10.10.10.1","port":22,"protocol":"tcp","service":"ssh","version":"OpenSSH 8.9"}],"credentials":[],"flags":[],"access":[],"notes":["SSH found"]}"#;
        apply_extraction(&db, "s1", json).unwrap();

        let hosts = db.list_hosts("s1").unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0]["ip"], "10.10.10.1");

        let ports = db.list_ports("s1", None).unwrap();
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0]["service"], "ssh");

        let notes = db.list_notes("s1").unwrap();
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn test_apply_extraction_credentials() {
        let db = Db::open_in_memory().unwrap();
        db.conn().execute("INSERT INTO sessions (id, name) VALUES ('s1', 'test')", []).unwrap();

        let json = r#"{"hosts":[],"ports":[],"credentials":[{"username":"admin","password":"secret","service":"ssh","host":"10.10.10.1"}],"flags":[],"access":[],"notes":[]}"#;
        apply_extraction(&db, "s1", json).unwrap();

        let creds = db.list_credentials("s1").unwrap();
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0]["username"], "admin");
    }
}
