use crate::error::Error;

const HOOK_EVENTS: &[&str] = &[
    "PostToolUse",
    "PostToolUseFailure",
    "SubagentStart",
    "SubagentStop",
    "UserPromptSubmit",
    "SessionStart",
    "SessionEnd",
    "Stop",
    "InstructionsLoaded",
    "ConfigChange",
];

/// Generate the hook script and install Claude Code hook configuration.
pub fn run() -> Result<(), Error> {
    let binary_path = find_binary_path()?;
    let hook_dir = ".claude/hooks";
    let hook_script = format!("{hook_dir}/redtrail-capture.sh");

    // Create hook directory
    std::fs::create_dir_all(hook_dir)?;

    // Write the hook script — accepts event type as $1
    let script =
        format!("#!/bin/bash\nexec {binary_path} ingest --event \"${{1:-PostToolUse}}\"\n");
    std::fs::write(&hook_script, &script)?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&hook_script, std::fs::Permissions::from_mode(0o755));
    }

    // Read or create settings.json
    let settings_path = ".claude/settings.json";
    let mut settings: serde_json::Value = match std::fs::read_to_string(settings_path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|e| Error::Config(format!("invalid settings.json: {e}")))?,
        Err(_) => serde_json::json!({}),
    };

    let hooks = settings
        .as_object_mut()
        .ok_or_else(|| Error::Config("settings.json is not an object".into()))?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| Error::Config("hooks is not an object".into()))?;

    // Register a hook for each event type
    for event in HOOK_EVENTS {
        let hook_config = serde_json::json!({
            "type": "command",
            "command": format!("bash {hook_script} {event}"),
            "async": true,
            "timeout": 5
        });

        let hook_entry = serde_json::json!({
            "hooks": [hook_config]
        });

        add_hook_entry(hooks_obj, event, hook_entry);
    }

    // Write settings back
    let formatted = serde_json::to_string_pretty(&settings)
        .map_err(|e| Error::Config(format!("serialize error: {e}")))?;
    std::fs::write(settings_path, formatted)?;

    eprintln!("Hook script: {hook_script}");
    eprintln!("Settings:    {settings_path}");
    eprintln!("Binary:      {binary_path}");
    eprintln!("Events:      {}", HOOK_EVENTS.join(", "));
    eprintln!("Agent capture hooks installed.");

    Ok(())
}

fn add_hook_entry(
    hooks_obj: &mut serde_json::Map<String, serde_json::Value>,
    event: &str,
    entry: serde_json::Value,
) {
    let arr = hooks_obj
        .entry(event)
        .or_insert_with(|| serde_json::json!([]));
    if let Some(arr) = arr.as_array_mut() {
        // Replace any existing redtrail hooks (upgrade path)
        arr.retain(|e| {
            !e.get("hooks")
                .and_then(|h| h.as_array())
                .is_some_and(|hooks| {
                    hooks.iter().any(|h| {
                        h.get("command")
                            .and_then(|c| c.as_str())
                            .is_some_and(|c| c.contains("redtrail"))
                    })
                })
        });
        arr.push(entry);
    }
}

fn find_binary_path() -> Result<String, Error> {
    // Try current exe first
    if let Ok(exe) = std::env::current_exe()
        && exe.exists()
    {
        return Ok(exe.to_string_lossy().to_string());
    }

    // Fall back to which
    let output = std::process::Command::new("which")
        .arg("redtrail")
        .output()
        .map_err(|e| Error::Config(format!("failed to find redtrail binary: {e}")))?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Ok(path);
        }
    }

    Err(Error::Config(
        "could not find redtrail binary path. Ensure it is installed and on PATH".into(),
    ))
}
