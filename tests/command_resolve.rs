use redtrail::workflows::command::resolve::{ResolvedCommand, resolve};

#[test]
fn resolves_session_as_builtin() {
    let cmd = resolve("session list");
    assert!(matches!(cmd, ResolvedCommand::Builtin { ref name, .. } if name == "session"));
}

#[test]
fn resolves_sql_as_builtin() {
    let cmd = resolve("sql SELECT * FROM hosts");
    assert!(matches!(cmd, ResolvedCommand::Builtin { ref name, .. } if name == "sql"));
}

#[test]
fn resolves_tools_as_builtin() {
    let cmd = resolve("tools");
    assert!(matches!(cmd, ResolvedCommand::Builtin { ref name, .. } if name == "tools"));
}

#[test]
fn resolves_background_shell() {
    let cmd = resolve("nmap -sV 10.10.10.1 &");
    assert!(matches!(cmd, ResolvedCommand::Shell { background: true, .. }));
}

#[test]
fn resolves_foreground_shell() {
    let cmd = resolve("nmap -sV 10.10.10.1");
    assert!(matches!(cmd, ResolvedCommand::Shell { background: false, .. }));
}

#[test]
fn resolves_env_as_builtin() {
    let cmd = resolve("env list");
    assert!(matches!(cmd, ResolvedCommand::Builtin { ref name, .. } if name == "env"));
}

#[test]
fn resolves_provider_as_builtin() {
    let cmd = resolve("provider set anthropic");
    assert!(matches!(cmd, ResolvedCommand::Builtin { ref name, .. } if name == "provider"));
}

#[test]
fn resolves_target_as_builtin() {
    let cmd = resolve("target set 10.10.10.1");
    assert!(matches!(cmd, ResolvedCommand::Builtin { ref name, .. } if name == "target"));
}

#[test]
fn resolves_jobs_as_builtin() {
    let cmd = resolve("jobs");
    assert!(matches!(cmd, ResolvedCommand::Builtin { ref name, .. } if name == "jobs"));
}

#[test]
fn resolves_ask_as_builtin() {
    let cmd = resolve("ask what should I try next?");
    assert!(matches!(cmd, ResolvedCommand::Builtin { ref name, .. } if name == "ask"));
}
