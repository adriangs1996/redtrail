use redtrail::workflows::command::pty::{PtyExecutor, PtyResult};

#[tokio::test]
async fn pty_runs_simple_command() {
    let result = PtyExecutor::run_foreground("echo hello").await.unwrap();
    assert_eq!(result.exit_code, Some(0));
    assert!(result.output.contains("hello"));
}

#[tokio::test]
async fn pty_captures_stderr() {
    let result = PtyExecutor::run_foreground("echo error >&2").await.unwrap();
    assert!(result.output.contains("error"));
}

#[tokio::test]
async fn pty_returns_exit_code() {
    let result = PtyExecutor::run_foreground("exit 42").await.unwrap();
    assert_eq!(result.exit_code, Some(42));
}

#[tokio::test]
async fn pty_handles_multiline_output() {
    let result = PtyExecutor::run_foreground("printf 'line1\\nline2\\nline3'").await.unwrap();
    let lines: Vec<&str> = result.output.lines().collect();
    assert!(lines.len() >= 3);
}

#[tokio::test]
async fn pty_inherits_env() {
    let result = PtyExecutor::run_foreground_with_env(
        "echo $TEST_VAR",
        vec![("TEST_VAR".into(), "pty_works".into())],
    ).await.unwrap();
    assert!(result.output.contains("pty_works"));
}
