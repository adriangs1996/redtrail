use std::process::Command;

#[test]
fn test_ask_help_shows_skill_flags() {
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["ask", "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--skill"), "should show --skill flag");
    assert!(stdout.contains("--no-skill"), "should show --no-skill flag");
}

#[test]
fn test_query_help_shows_skill_flags() {
    let out = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["query", "--help"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--skill"), "should show --skill flag");
    assert!(stdout.contains("--no-skill"), "should show --no-skill flag");
}
