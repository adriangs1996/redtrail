use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::Read;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};
use crate::error::Error;

pub struct PtyResult {
    pub output: String,
    pub exit_code: Option<i32>,
}

pub struct PtyExecutor;

impl PtyExecutor {
    pub async fn run_foreground(command: &str) -> Result<PtyResult, Error> {
        Self::run_foreground_with_env(command, vec![]).await
    }

    pub async fn run_foreground_with_env(
        command: &str,
        env: Vec<(String, String)>,
    ) -> Result<PtyResult, Error> {
        let cmd = command.to_string();
        let env_clone = env.clone();

        tokio::task::spawn_blocking(move || {
            let pty_system = NativePtySystem::default();
            let pair = pty_system.openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            }).map_err(|e| Error::Io(std::io::Error::other(
                e.to_string()
            )))?;

            let mut cmd_builder = CommandBuilder::new("sh");
            cmd_builder.arg("-c");
            cmd_builder.arg(&cmd);
            for (key, val) in &env_clone {
                cmd_builder.env(key, val);
            }

            let mut child = pair.slave.spawn_command(cmd_builder)
                .map_err(|e| Error::Io(std::io::Error::other(
                    e.to_string()
                )))?;

            drop(pair.slave);

            let mut reader = pair.master.try_clone_reader()
                .map_err(|e| Error::Io(std::io::Error::other(
                    e.to_string()
                )))?;

            let mut output = String::new();
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buf[..n]);
                        output.push_str(&chunk);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(_) => break,
                }
            }

            let status = child.wait()
                .map_err(|e| Error::Io(std::io::Error::other(
                    e.to_string()
                )))?;

            let exit_code = Some(status.exit_code() as i32);
            let cleaned = output.replace('\r', "");

            Ok(PtyResult { output: cleaned, exit_code })
        }).await.map_err(|e| Error::Io(std::io::Error::other(
            e.to_string()
        )))?
    }

    pub fn spawn_background(
        command: &str,
        env: Vec<(String, String)>,
    ) -> Result<PtyBackgroundHandle, Error> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }).map_err(|e| Error::Io(std::io::Error::other(
            e.to_string()
        )))?;

        let mut cmd_builder = CommandBuilder::new("sh");
        cmd_builder.arg("-c");
        cmd_builder.arg(command);
        for (key, val) in &env {
            cmd_builder.env(key, val);
        }

        let child = pair.slave.spawn_command(cmd_builder)
            .map_err(|e| Error::Io(std::io::Error::other(
                e.to_string()
            )))?;

        drop(pair.slave);

        let reader = pair.master.try_clone_reader()
            .map_err(|e| Error::Io(std::io::Error::other(
                e.to_string()
            )))?;

        Ok(PtyBackgroundHandle {
            reader: Box::new(reader),
            child,
            master: pair.master,
        })
    }

    pub fn spawn_streaming(
        command: &str,
        env: Vec<(String, String)>,
    ) -> Result<(mpsc::UnboundedReceiver<String>, oneshot::Receiver<i32>, PtyKillHandle), Error> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }).map_err(|e| Error::Io(std::io::Error::other(
            e.to_string()
        )))?;

        let mut cmd_builder = CommandBuilder::new("sh");
        cmd_builder.arg("-c");
        cmd_builder.arg(command);
        for (key, val) in &env {
            cmd_builder.env(key, val);
        }

        let child = pair.slave.spawn_command(cmd_builder)
            .map_err(|e| Error::Io(std::io::Error::other(
                e.to_string()
            )))?;

        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()
            .map_err(|e| Error::Io(std::io::Error::other(
                e.to_string()
            )))?;

        let child_arc: Arc<Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>> =
            Arc::new(Mutex::new(Some(child)));
        let child_for_thread = child_arc.clone();

        let (line_tx, line_rx) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = oneshot::channel();

        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut partial = String::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buf[..n]);
                        let cleaned = chunk.replace('\r', "");
                        partial.push_str(&cleaned);
                        while let Some(pos) = partial.find('\n') {
                            let line = partial[..pos].to_string();
                            partial = partial[pos + 1..].to_string();
                            let _ = line_tx.send(line);
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(_) => break,
                }
            }
            if !partial.is_empty() {
                let _ = line_tx.send(partial);
            }
            let exit_code = if let Some(mut child) = child_for_thread.lock().unwrap().take() {
                match child.wait() {
                    Ok(status) => status.exit_code() as i32,
                    Err(_) => -1,
                }
            } else {
                -1
            };
            let _ = done_tx.send(exit_code);
        });

        let kill_handle = PtyKillHandle {
            child: child_arc,
            _master: pair.master,
        };

        Ok((line_rx, done_rx, kill_handle))
    }
}

pub struct PtyKillHandle {
    child: Arc<Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>>,
    _master: Box<dyn portable_pty::MasterPty + Send>,
}

impl PtyKillHandle {
    pub fn kill(&self) {
        if let Some(ref mut child) = *self.child.lock().unwrap() {
            let _ = child.kill();
        }
    }
}

pub struct PtyBackgroundHandle {
    pub reader: Box<dyn Read + Send>,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
    pub master: Box<dyn portable_pty::MasterPty + Send>,
}

impl PtyBackgroundHandle {
    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }
}

