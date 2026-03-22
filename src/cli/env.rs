use crate::config::Config;
use crate::db::SessionOps;
use crate::error::Error;

const COMMAND_ALIASES: &[(&str, &str)] = &[
    ("kb", "kb"),
    ("st", "status"),
    ("theory", "hypothesis"),
    ("ev", "evidence"),
    ("sess", "session"),
    ("scope", "scope"),
    ("conf", "config"),
    ("eat", "ingest"),
    ("rep", "report"),
    ("skill", "skill"),
    ("ask", "ask"),
    ("q", "query"),
    ("sql", "sql"),
];

const LPORT_DEFAULT: u16 = 4444;

pub fn run(db: &impl SessionOps, session_id: &str, config: &Config) -> Result<(), Error> {
    let session = db.get_session(session_id)?;
    let name = session["name"].as_str().unwrap_or("");
    let target = session["target"].as_str().unwrap_or("");
    let scope = session["scope"].as_str().unwrap_or("");
    let workspace = session["workspace_path"].as_str().unwrap_or("");
    let tool_aliases = &config.tools.aliases;

    let mut out = String::with_capacity(2048);

    out.push_str(&format!("export RT_SESSION='{session_id}';\n"));
    out.push_str(&format!("export RT_WORKSPACE='{workspace}';\n"));
    out.push_str(&format!("export TARGET='{target}';\n"));
    out.push_str(&format!("export RHOST='{target}';\n"));
    out.push_str(&format!("export SCOPE='{scope}';\n"));

    if !target.is_empty() {
        out.push_str(&format!(
            "export LHOST=$(ip route get {target} 2>/dev/null | awk '/src/ {{print $5; exit}}');\n"
        ));
    }

    out.push_str(&format!("export LPORT={LPORT_DEFAULT};\n"));

    for tool in tool_aliases {
        out.push_str(&format!("alias {tool}='rt {tool}';\n"));
    }

    for (short, full) in COMMAND_ALIASES {
        out.push_str(&format!("alias {short}='rt {full}';\n"));
    }

    let phase = session["phase"].as_str().unwrap_or("L0");
    out.push_str(r#"[[ -z "$_RT_OLD_PROMPT" ]] && export _RT_OLD_PROMPT="$PROMPT";"#);
    out.push('\n');
    out.push_str(&format!(
        r#"_rt_precmd() {{ local p="%F{{red}}(rt:{name})%f"; [[ -n "$TARGET" ]] && p+=" %F{{yellow}}$TARGET%f"; [[ -n "$LHOST" ]] && p+=" %F{{cyan}}⇄$LHOST%f"; p+=" %F{{magenta}}[{phase}]%f %F{{blue}}%~%f"; PROMPT="${{p}} %F{{green}}❯%f "; }};"#
    ));
    out.push('\n');
    out.push_str("autoload -Uz add-zsh-hook;\n");
    out.push_str("add-zsh-hook precmd _rt_precmd;\n");

    out.push_str("rt_deactivate() { ");
    for tool in tool_aliases {
        out.push_str(&format!("unalias {tool} 2>/dev/null; "));
    }
    for (short, _) in COMMAND_ALIASES {
        out.push_str(&format!("unalias {short} 2>/dev/null; "));
    }
    out.push_str("add-zsh-hook -d precmd _rt_precmd; ");
    out.push_str("unset -f _rt_precmd; ");
    out.push_str("unset RT_SESSION RT_WORKSPACE TARGET SCOPE RHOST LHOST LPORT; ");
    out.push_str(r#"PROMPT="${_RT_OLD_PROMPT:-%(?.%F{green}.%F{red})❯%f }"; "#);
    out.push_str("unset _RT_OLD_PROMPT; ");
    out.push_str("unset -f rt_deactivate; ");
    out.push_str("};\n");

    print!("{out}");
    Ok(())
}

pub fn deactivate() -> Result<(), Error> {
    let ctx = crate::resolve::resolve_global()?;
    let config = Config::resolved_global(&ctx.conn)?;
    let tool_aliases = &config.tools.aliases;

    let mut out = String::with_capacity(1024);

    for tool in tool_aliases {
        out.push_str(&format!("unalias {tool} 2>/dev/null;\n"));
    }
    for (short, _) in COMMAND_ALIASES {
        out.push_str(&format!("unalias {short} 2>/dev/null;\n"));
    }
    out.push_str("add-zsh-hook -d precmd _rt_precmd 2>/dev/null;\n");
    out.push_str("unset -f _rt_precmd 2>/dev/null;\n");
    out.push_str("unset RT_SESSION RT_WORKSPACE TARGET SCOPE RHOST LHOST LPORT;\n");
    out.push_str(r#"PROMPT="${_RT_OLD_PROMPT:-%(?.%F{green}.%F{red})❯%f }";"#);
    out.push('\n');
    out.push_str("unset _RT_OLD_PROMPT;\n");
    out.push_str("unset -f rt_deactivate;\n");

    print!("{out}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SessionOps;
    use crate::error::Error;
    use serde_json::json;

    struct FakeDb {
        session: serde_json::Value,
    }

    impl SessionOps for FakeDb {
        fn active_session_id(&self, _wp: &str) -> Result<String, Error> {
            Ok(self.session["id"].as_str().unwrap().to_string())
        }
        fn create_session(&self, _id: &str, _name: &str, _wp: &str, _target: Option<&str>, _scope: Option<&str>, _goal: &str) -> Result<(), Error> { Ok(()) }
        fn deactivate_session(&self, _wp: &str) -> Result<(), Error> { Ok(()) }
        fn activate_session(&self, _sid: &str) -> Result<(), Error> { Ok(()) }
        fn get_session(&self, _sid: &str) -> Result<serde_json::Value, Error> { Ok(self.session.clone()) }
        fn load_scope(&self, _sid: &str) -> Result<Option<String>, Error> { Ok(None) }
        fn status_summary(&self, _sid: &str) -> Result<serde_json::Value, Error> { Ok(json!({})) }
    }

    fn capture_run(target: &str, scope: &str) -> String {
        let db = FakeDb {
            session: json!({
                "id": "sess-001",
                "name": "htb-box",
                "workspace_path": "/tmp/pentest",
                "target": target,
                "scope": scope,
                "goal": "general",
                "phase": "L2",
            }),
        };
        let config = Config::default();
        let mut buf = Vec::new();
        write_env(&db, "sess-001", &config, &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    fn write_env(db: &impl SessionOps, session_id: &str, config: &Config, w: &mut impl std::io::Write) -> Result<(), Error> {
        let session = db.get_session(session_id)?;
        let name = session["name"].as_str().unwrap_or("");
        let target = session["target"].as_str().unwrap_or("");
        let scope = session["scope"].as_str().unwrap_or("");
        let workspace = session["workspace_path"].as_str().unwrap_or("");
        let tool_aliases = &config.tools.aliases;

        writeln!(w, "export RT_SESSION='{session_id}';")?;
        writeln!(w, "export RT_WORKSPACE='{workspace}';")?;
        writeln!(w, "export TARGET='{target}';")?;
        writeln!(w, "export RHOST='{target}';")?;
        writeln!(w, "export SCOPE='{scope}';")?;

        if !target.is_empty() {
            writeln!(w, "export LHOST=$(ip route get {target} 2>/dev/null | awk '/src/ {{print $5; exit}}');")?;
        }

        writeln!(w, "export LPORT={LPORT_DEFAULT};")?;

        for tool in tool_aliases {
            writeln!(w, "alias {tool}='rt {tool}';")?;
        }
        for (short, full) in COMMAND_ALIASES {
            writeln!(w, "alias {short}='rt {full}';")?;
        }

        let phase = session["phase"].as_str().unwrap_or("L0");
        writeln!(w, r#"[[ -z "$_RT_OLD_PROMPT" ]] && export _RT_OLD_PROMPT="$PROMPT";"#)?;
        write!(w, r#"_rt_precmd() {{ local p="%F{{red}}(rt:{name})%f"; [[ -n "$TARGET" ]] && p+=" %F{{yellow}}$TARGET%f"; [[ -n "$LHOST" ]] && p+=" %F{{cyan}}⇄$LHOST%f"; p+=" %F{{magenta}}[{phase}]%f %F{{blue}}%~%f"; PROMPT="${{p}} %F{{green}}❯%f "; }};"#)?;
        writeln!(w)?;
        writeln!(w, "autoload -Uz add-zsh-hook;")?;
        writeln!(w, "add-zsh-hook precmd _rt_precmd;")?;

        write!(w, "rt_deactivate() {{ ")?;
        for tool in tool_aliases {
            write!(w, "unalias {tool} 2>/dev/null; ")?;
        }
        for (short, _) in COMMAND_ALIASES {
            write!(w, "unalias {short} 2>/dev/null; ")?;
        }
        write!(w, "add-zsh-hook -d precmd _rt_precmd; ")?;
        write!(w, "unset -f _rt_precmd; ")?;
        write!(w, "unset RT_SESSION RT_WORKSPACE TARGET SCOPE RHOST LHOST LPORT; ")?;
        write!(w, r#"PROMPT="${{_RT_OLD_PROMPT:-%(?.%F{{green}}.%F{{red}})❯%f }}"; "#)?;
        write!(w, "unset _RT_OLD_PROMPT; ")?;
        write!(w, "unset -f rt_deactivate; ")?;
        writeln!(w, "}};")?;

        Ok(())
    }

    #[test]
    fn env_contains_target_and_rhost() {
        let out = capture_run("10.10.10.1", "10.10.10.0/24");
        assert!(out.contains("export TARGET='10.10.10.1'"));
        assert!(out.contains("export RHOST='10.10.10.1'"));
    }

    #[test]
    fn env_contains_scope() {
        let out = capture_run("10.10.10.1", "10.10.10.0/24");
        assert!(out.contains("export SCOPE='10.10.10.0/24'"));
    }

    #[test]
    fn env_contains_lport() {
        let out = capture_run("10.10.10.1", "");
        assert!(out.contains("export LPORT=4444"));
    }

    #[test]
    fn env_contains_lhost_when_target_set() {
        let out = capture_run("10.10.10.1", "");
        assert!(out.contains("export LHOST=$(ip route get 10.10.10.1"));
    }

    #[test]
    fn env_no_lhost_when_target_empty() {
        let out = capture_run("", "");
        assert!(!out.contains("export LHOST"));
    }

    #[test]
    fn env_contains_tool_aliases() {
        let out = capture_run("10.10.10.1", "");
        assert!(out.contains("alias nmap='rt nmap'"));
    }

    #[test]
    fn env_contains_command_aliases() {
        let out = capture_run("10.10.10.1", "");
        assert!(out.contains("alias kb='rt kb'"));
        assert!(out.contains("alias st='rt status'"));
    }

    #[test]
    fn env_contains_precmd() {
        let out = capture_run("10.10.10.1", "");
        assert!(out.contains("_rt_precmd"));
        assert!(out.contains("add-zsh-hook precmd _rt_precmd"));
    }

    #[test]
    fn env_contains_deactivate() {
        let out = capture_run("10.10.10.1", "");
        assert!(out.contains("rt_deactivate()"));
        assert!(out.contains("unset RT_SESSION RT_WORKSPACE TARGET SCOPE RHOST LHOST LPORT"));
    }

    #[test]
    fn env_contains_session_vars() {
        let out = capture_run("10.10.10.1", "");
        assert!(out.contains("export RT_SESSION='sess-001'"));
        assert!(out.contains("export RT_WORKSPACE='/tmp/pentest'"));
    }
}
