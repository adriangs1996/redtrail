use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq)]
pub enum ShellStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
pub enum ShellEvent {
    Line {
        id: u64,
        text: String,
        stream: ShellStream,
    },
    Exit {
        id: u64,
        code: Option<i32>,
    },
}

pub fn spawn_shell(
    command: &str,
    id: u64,
    tx: mpsc::Sender<ShellEvent>,
) -> std::io::Result<(Child, tokio::process::ChildStdin)> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let stdin = child.stdin.take().expect("stdin was configured as piped");

    let stdout = child.stdout.take().expect("stdout was configured as piped");
    let stderr = child.stderr.take().expect("stderr was configured as piped");

    let tx1 = tx.clone();
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx1
                .send(ShellEvent::Line {
                    id,
                    text: line,
                    stream: ShellStream::Stdout,
                })
                .await;
        }
    });

    let tx2 = tx.clone();
    tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx2
                .send(ShellEvent::Line {
                    id,
                    text: line,
                    stream: ShellStream::Stderr,
                })
                .await;
        }
    });

    Ok((child, stdin))
}

pub fn spawn_exit_waiter(mut child: Child, id: u64, tx: mpsc::Sender<ShellEvent>) {
    tokio::spawn(async move {
        let status = child.wait().await;
        let code = status.ok().and_then(|s| s.code());
        let _ = tx.send(ShellEvent::Exit { id, code }).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spawn_shell_echo() {
        let (tx, mut rx) = mpsc::channel(64);
        let (child, _stdin) = spawn_shell("echo hello", 1, tx.clone()).unwrap();
        spawn_exit_waiter(child, 1, tx);

        let mut got_line = false;
        let mut got_exit = false;

        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
        while let Ok(Some(event)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match event {
                ShellEvent::Line { text, .. } => {
                    if text == "hello" {
                        got_line = true;
                    }
                }
                ShellEvent::Exit { code, .. } => {
                    assert_eq!(code, Some(0));
                    got_exit = true;
                    break;
                }
            }
        }

        assert!(got_line, "should have received 'hello' line");
        assert!(got_exit, "should have received exit event");
    }

    #[tokio::test]
    async fn test_spawn_shell_stderr() {
        let (tx, mut rx) = mpsc::channel(64);
        let (child, _stdin) = spawn_shell("echo err >&2", 2, tx.clone()).unwrap();
        spawn_exit_waiter(child, 2, tx);

        let mut got_stderr = false;
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
        while let Ok(Some(event)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match event {
                ShellEvent::Line { stream, .. } => {
                    if stream == ShellStream::Stderr {
                        got_stderr = true;
                    }
                }
                ShellEvent::Exit { .. } => break,
            }
        }

        assert!(got_stderr, "should have received stderr line");
    }

    #[tokio::test]
    async fn test_spawn_shell_nonzero_exit() {
        let (tx, mut rx) = mpsc::channel(64);
        let (child, _stdin) = spawn_shell("exit 42", 3, tx.clone()).unwrap();
        spawn_exit_waiter(child, 3, tx);

        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
        while let Ok(Some(event)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            if let ShellEvent::Exit { code, .. } = event {
                assert_eq!(code, Some(42));
                return;
            }
        }
        panic!("should have received exit event");
    }
}
