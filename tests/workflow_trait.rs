use redtrail::workflows::{Workflow, ShellCommand, CommandOutput};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn shell_command_defaults() {
    let cmd = ShellCommand {
        raw: "nmap -sV 10.10.10.1".into(),
        program: "nmap".into(),
        args: vec!["-sV".into(), "10.10.10.1".into()],
        env_overrides: HashMap::new(),
        working_dir: PathBuf::from("/tmp"),
    };
    assert_eq!(cmd.program, "nmap");
    assert_eq!(cmd.args.len(), 2);
}

#[test]
fn command_output_defaults() {
    let out = CommandOutput {
        lines: vec![],
        exit_code: Some(0),
        duration: Duration::from_secs(1),
    };
    assert_eq!(out.exit_code, Some(0));
    assert!(out.lines.is_empty());
}
