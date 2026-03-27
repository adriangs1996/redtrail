use redtrail::core::capture;

#[test]
fn parse_simple_command() {
    let parsed = capture::parse_command("git status");
    assert_eq!(parsed.binary, "git");
    assert_eq!(parsed.subcommand.as_deref(), Some("status"));
    assert!(parsed.args.is_empty());
    assert!(parsed.flags.is_empty());
}

#[test]
fn parse_command_with_flags() {
    let parsed = capture::parse_command("git commit -m 'initial commit' --amend");
    assert_eq!(parsed.binary, "git");
    assert_eq!(parsed.subcommand.as_deref(), Some("commit"));
    assert!(parsed.flags.contains_key("-m"), "should capture -m flag");
    assert!(parsed.flags.contains_key("--amend"), "should capture --amend flag");
}

#[test]
fn parse_command_with_positional_args() {
    let parsed = capture::parse_command("docker build -t myapp .");
    assert_eq!(parsed.binary, "docker");
    assert_eq!(parsed.subcommand.as_deref(), Some("build"));
    assert!(parsed.flags.contains_key("-t"));
    // myapp and . are both positional args since we don't consume flag values
    assert!(parsed.args.contains(&".".to_string()));
}

#[test]
fn parse_empty_command() {
    let parsed = capture::parse_command("");
    assert_eq!(parsed.binary, "");
    assert!(parsed.subcommand.is_none());
}

#[test]
fn parse_command_no_subcommand() {
    let parsed = capture::parse_command("ls -la /tmp");
    assert_eq!(parsed.binary, "ls");
    assert!(parsed.subcommand.is_none(), "ls doesn't have subcommands");
    assert!(parsed.flags.contains_key("-la"));
    assert!(parsed.args.contains(&"/tmp".to_string()));
}

#[test]
fn parse_command_with_quoted_args() {
    let parsed = capture::parse_command("grep -r 'hello world' src/");
    assert_eq!(parsed.binary, "grep");
    assert!(parsed.args.contains(&"hello world".to_string()));
    assert!(parsed.args.contains(&"src/".to_string()));
}

#[test]
fn parsed_to_json_fields() {
    let parsed = capture::parse_command("git push origin main --force");
    assert_eq!(parsed.binary, "git");
    assert_eq!(parsed.subcommand.as_deref(), Some("push"));

    let args_json = serde_json::to_string(&parsed.args).unwrap();
    let flags_json = serde_json::to_string(&parsed.flags).unwrap();
    assert!(args_json.contains("origin"));
    assert!(flags_json.contains("--force"));
}
