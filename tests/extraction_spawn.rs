use std::process::Command;

fn rt_bin() -> String {
    env!("CARGO_BIN_EXE_rt").to_string()
}

fn run_rt(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(rt_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run rt")
}

#[test]
fn test_pipeline_extract_subcommand_runs() {
    let tmp = tempfile::tempdir().unwrap();

    run_rt(tmp.path(), &["init", "--target", "10.10.10.1"]);

    run_rt(tmp.path(), &[
        "sql",
        "INSERT INTO command_history (session_id, command, tool, exit_code, duration_ms, output) SELECT id, 'echo hello', 'echo', 0, 100, 'hello' FROM sessions LIMIT 1",
    ]);

    let id_output = run_rt(tmp.path(), &["sql", "--json", "SELECT id FROM command_history ORDER BY id DESC LIMIT 1"]);
    let id_str = String::from_utf8_lossy(&id_output.stdout);
    let v: serde_json::Value = serde_json::from_str(id_str.trim()).unwrap_or_default();
    let cmd_id = v[0]["id"].as_i64()
        .or_else(|| v[0]["id"].as_str().and_then(|s| s.parse::<i64>().ok()))
        .expect("could not get cmd_id");

    Command::new(rt_bin())
        .args(["pipeline", "extract", &cmd_id.to_string()])
        .current_dir(tmp.path())
        .env_remove("ANTHROPIC_API_KEY")
        .output()
        .expect("failed to run rt");

    let status_output = run_rt(tmp.path(), &[
        "sql", "--json",
        &format!("SELECT extraction_status FROM command_history WHERE id = {cmd_id}"),
    ]);
    let status_str = String::from_utf8_lossy(&status_output.stdout);
    let sv: serde_json::Value = serde_json::from_str(status_str.trim()).unwrap_or_default();
    let status = sv[0]["extraction_status"].as_str().unwrap_or("unknown");

    assert!(
        status == "failed" || status == "skipped",
        "expected failed or skipped without API key, got: {status}"
    );
}

#[test]
fn test_pipeline_extract_help() {
    let output = Command::new(rt_bin())
        .args(["pipeline", "--help"])
        .output()
        .expect("failed to run rt pipeline --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("extract"), "help should list extract subcommand");
}
