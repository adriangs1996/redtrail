use redtrail::core::capture;

#[test]
fn detects_claude_code_from_env() {
    let mut env = std::collections::HashMap::new();
    env.insert("CLAUDE_CODE".to_string(), "1".to_string());
    assert_eq!(capture::detect_source(&env, None), "claude_code");
}

#[test]
fn detects_cursor_from_env() {
    let mut env = std::collections::HashMap::new();
    env.insert("CURSOR_SESSION_ID".to_string(), "abc".to_string());
    assert_eq!(capture::detect_source(&env, None), "cursor");
}

#[test]
fn detects_codex_from_env() {
    let mut env = std::collections::HashMap::new();
    env.insert("CODEX_SESSION".to_string(), "xyz".to_string());
    assert_eq!(capture::detect_source(&env, None), "codex");
}

#[test]
fn detects_agent_from_parent_process() {
    let env = std::collections::HashMap::new();
    assert_eq!(capture::detect_source(&env, Some("claude")), "claude_code");
    assert_eq!(capture::detect_source(&env, Some("cursor")), "cursor");
    assert_eq!(capture::detect_source(&env, Some("aider")), "aider");
}

#[test]
fn defaults_to_human_when_no_signals() {
    let env = std::collections::HashMap::new();
    assert_eq!(capture::detect_source(&env, None), "human");
}

#[test]
fn unknown_agent_parent() {
    let env: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    // A parent process that looks automated but isn't a known agent
    assert_eq!(capture::detect_source(&env, Some("bash")), "human");
}

#[test]
fn is_automated_flag() {
    assert!(!capture::is_automated("human"));
    assert!(capture::is_automated("claude_code"));
    assert!(capture::is_automated("cursor"));
    assert!(capture::is_automated("unknown_agent"));
}
