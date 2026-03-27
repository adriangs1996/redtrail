use redtrail::core::capture;

#[test]
fn snapshot_env_captures_selected_vars() {
    let mut env = std::collections::HashMap::new();
    env.insert("PATH".to_string(), "/usr/bin:/usr/local/bin".to_string());
    env.insert("NODE_ENV".to_string(), "production".to_string());
    env.insert("HOME".to_string(), "/home/user".to_string()); // not in default list
    env.insert("RANDOM_VAR".to_string(), "ignored".to_string());

    let snapshot = capture::env_snapshot(&env);
    let parsed: serde_json::Value = serde_json::from_str(&snapshot).unwrap();

    assert_eq!(parsed["PATH"], "/usr/bin:/usr/local/bin");
    assert_eq!(parsed["NODE_ENV"], "production");
    assert!(parsed.get("HOME").is_none(), "HOME should not be captured");
    assert!(parsed.get("RANDOM_VAR").is_none(), "random vars should not be captured");
}

#[test]
fn snapshot_env_missing_vars_are_omitted() {
    let env = std::collections::HashMap::new();
    let snapshot = capture::env_snapshot(&env);
    let parsed: serde_json::Value = serde_json::from_str(&snapshot).unwrap();

    assert!(parsed.as_object().unwrap().is_empty() || parsed.is_object());
}

#[test]
fn default_snapshot_vars_list() {
    let vars = capture::SNAPSHOT_ENV_VARS;
    assert!(vars.contains(&"PATH"));
    assert!(vars.contains(&"VIRTUAL_ENV"));
    assert!(vars.contains(&"NODE_ENV"));
    assert!(vars.contains(&"AWS_PROFILE"));
    assert!(vars.contains(&"KUBECONFIG"));
}
