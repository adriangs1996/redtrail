use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};

use crossterm::{
    event::{KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Widget,
};
use tokio::sync::{mpsc, oneshot};
use crate::agent::llm::{ChatMessage, LlmProvider};
use crate::agent::tools::ToolRegistry;
use crate::db_v2::DbV2;
use crate::error::Error;
use crate::middleware::builtin::logger::LoggerListener;
use crate::middleware::events::{EventBus, ShellEvent};
use crate::middleware::pipeline::Pipeline;
use crate::workflows::chat::{ChatInput, ChatWorkflow, RecentCommand};
use crate::workflows::command::jobs::JobTable;
use crate::workflows::command::pty::PtyKillHandle;
use crate::workflows::command::resolve::{self, ResolvedCommand};
use crate::workflows::query::{QueryInput, QueryWorkflow};
use crate::workflows::session::{SessionContext, SessionWorkflow};
use crate::workflows::{Block, BlockContent, BlockStatus, ShellOutputLine, ShellOutputStream};
use crate::completion::CompletionEngine;
use crate::tui::prompt::render_prompt;

use super::channel::InputMode;
use super::events::EventHandler;
use super::widgets::InputBar;
use super::widgets::block_view::BlockView;
use super::widgets::status_bar::{StatusBar, StatusBarData};

struct RunningCommand {
    block_idx: usize,
    job_id: u32,
    input_history_id: Option<i64>,
    output_rx: mpsc::UnboundedReceiver<String>,
    done_rx: oneshot::Receiver<i32>,
    kill_handle: PtyKillHandle,
}

struct RunningChat {
    block_idx: usize,
    user_message: String,
    is_chat: bool,
    token_rx: mpsc::UnboundedReceiver<String>,
    result_rx: oneshot::Receiver<Result<crate::workflows::chat::ChatResult, Error>>,
}

pub struct App {
    should_quit: bool,
    blocks: Vec<Block>,
    focused_block: Option<usize>,
    block_scroll: u16,
    input: InputBar,
    session: SessionContext,
    db: Arc<Mutex<DbV2>>,
    jobs: JobTable,
    pipeline: Pipeline,
    event_bus: EventBus,
    completion: CompletionEngine,
    next_block_id: usize,
    chat_history: Vec<ChatMessage>,
    provider: Option<Arc<dyn LlmProvider>>,
    tools: Option<Arc<ToolRegistry>>,
    running_cmds: Vec<RunningCommand>,
    running_chats: Vec<RunningChat>,
    blocks_viewport_h: u16,
}

impl App {
    pub fn new(
        session: SessionContext,
        db: Arc<Mutex<DbV2>>,
        provider: Option<Arc<dyn LlmProvider>>,
        tools: Option<Arc<ToolRegistry>>,
    ) -> Self {
        let mut event_bus = EventBus::new();
        let logger = Arc::new(LoggerListener::new(db.clone()));
        event_bus.add(logger);
        Self {
            should_quit: false,
            blocks: vec![],
            focused_block: None,
            block_scroll: 0,
            input: InputBar::default(),
            session,
            db,
            jobs: JobTable::new(),
            pipeline: Pipeline::new(),
            event_bus,
            completion: CompletionEngine::new(),
            next_block_id: 0,
            chat_history: vec![],
            provider,
            tools,
            running_cmds: vec![],
            running_chats: vec![],
            blocks_viewport_h: 0,
        }
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        self.load_persisted_state();

        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
            crossterm::cursor::MoveTo(0, 0),
            EnterAlternateScreen
        )?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let events = EventHandler::new(33);

        while !self.should_quit {
            self.drain_running();
            terminal.draw(|f| self.render(f))?;

            match events.next().await {
                super::events::Event::Key(key) => {
                    self.handle_key(key);
                }
                super::events::Event::Resize(_, _) | super::events::Event::Tick => {}
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }

    fn render(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(3),
            ])
            .split(f.area());

        let data = StatusBarData {
            session_name: self.session.name.clone(),
            running_jobs: self.jobs.running_count(),
            host_count: 0,
            cred_count: 0,
            flag_count: 0,
        };
        StatusBar::new(&data).render(chunks[0], f.buffer_mut());

        let blocks_area = chunks[1];
        self.blocks_viewport_h = blocks_area.height;
        let max_block_h = (blocks_area.height / 2).max(3);
        let gap: u16 = 1;

        let block_heights: Vec<u16> = self.blocks.iter().map(|b| {
            if b.collapsed { 3 } else { ((b.content.line_count() as u16) + 2).min(max_block_h) }
        }).collect();
        let total_h: u16 = block_heights.iter().sum::<u16>()
            + if self.blocks.len() > 1 { (self.blocks.len() as u16 - 1) * gap } else { 0 };

        if total_h > blocks_area.height {
            self.block_scroll = self.block_scroll
                .min(total_h.saturating_sub(blocks_area.height));
        } else {
            self.block_scroll = 0;
        }

        let mut cumulative: u16 = 0;
        let mut y = blocks_area.y;
        for (i, block) in self.blocks.iter().enumerate() {
            let h = block_heights[i];
            let slot = h + if i + 1 < self.blocks.len() { gap } else { 0 };
            if cumulative + slot <= self.block_scroll {
                cumulative += slot;
                continue;
            }
            let clip_top = self.block_scroll.saturating_sub(cumulative);
            cumulative += slot;
            let visible_h = h.saturating_sub(clip_top);
            let remaining = (blocks_area.y + blocks_area.height).saturating_sub(y);
            if remaining == 0 { break; }
            let render_h = visible_h.min(remaining);
            let area = Rect { x: blocks_area.x, y, width: blocks_area.width, height: render_h };
            let focused = self.focused_block == Some(i);
            BlockView::new(block, focused).render(area, f.buffer_mut());
            y += render_h + if remaining > render_h { gap.min(remaining - render_h) } else { 0 };
        }

        let vars = self.build_prompt_vars();
        let prompt = render_prompt(&self.session.prompt_template, &vars);
        self.input.set_prompt(&prompt);
        self.input.render(f, chunks[2], InputMode::Terminal);
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                if self.running_cmds.is_empty() {
                    self.should_quit = true;
                } else {
                    for rc in &self.running_cmds {
                        rc.kill_handle.kill();
                    }
                }
            }
            (KeyCode::Up, KeyModifiers::NONE) => {
                if let Some(idx) = self.focused_block {
                    if let Some(block) = self.blocks.get_mut(idx) {
                        let cur = if block.content_scroll == u16::MAX {
                            (block.content.line_count() as u16).saturating_sub(1)
                        } else {
                            block.content_scroll
                        };
                        block.content_scroll = cur.saturating_sub(3);
                    }
                } else {
                    self.input.history_up();
                }
            }
            (KeyCode::Down, KeyModifiers::NONE) => {
                if let Some(idx) = self.focused_block {
                    if let Some(block) = self.blocks.get_mut(idx) {
                        let cur = if block.content_scroll == u16::MAX {
                            u16::MAX
                        } else {
                            block.content_scroll.saturating_add(3)
                        };
                        block.content_scroll = cur;
                    }
                } else {
                    self.input.history_down();
                }
            }
            (KeyCode::PageUp, _) => {
                if let Some(idx) = self.focused_block {
                    if let Some(block) = self.blocks.get_mut(idx) {
                        let cur = if block.content_scroll == u16::MAX {
                            (block.content.line_count() as u16).saturating_sub(1)
                        } else {
                            block.content_scroll
                        };
                        block.content_scroll = cur.saturating_sub(15);
                    }
                } else {
                    self.block_scroll = self.block_scroll.saturating_sub(15);
                }
            }
            (KeyCode::PageDown, _) => {
                if let Some(idx) = self.focused_block {
                    if let Some(block) = self.blocks.get_mut(idx) {
                        let cur = if block.content_scroll == u16::MAX {
                            u16::MAX
                        } else {
                            block.content_scroll.saturating_add(15)
                        };
                        block.content_scroll = cur;
                    }
                } else {
                    self.block_scroll = self.block_scroll.saturating_add(15);
                }
            }
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                if let Some(idx) = self.focused_block {
                    if idx > 0 {
                        self.focused_block = Some(idx - 1);
                    }
                } else if !self.blocks.is_empty() {
                    self.focused_block = Some(self.blocks.len() - 1);
                }
                self.scroll_to_focused();
            }
            (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                if let Some(idx) = self.focused_block {
                    if idx + 1 < self.blocks.len() {
                        self.focused_block = Some(idx + 1);
                    } else {
                        self.focused_block = None;
                    }
                }
                self.scroll_to_focused();
            }
            (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
                if let Some(idx) = self.focused_block
                    && let Some(block) = self.blocks.get_mut(idx) {
                        block.collapsed = !block.collapsed;
                    }
            }
            (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
                if let Some(idx) = self.focused_block
                    && let Some(block) = self.blocks.get(idx)
                        && let Some(job_id) = block.job_id {
                            if let Some(rc) = self.running_cmds.iter().find(|r| r.job_id == job_id) {
                                rc.kill_handle.kill();
                            }
                            self.jobs.finish(job_id, -1);
                        }
            }
            (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
                self.blocks.retain(|b| matches!(b.status, BlockStatus::Running));
                self.focused_block = None;
            }
            (KeyCode::Enter, KeyModifiers::NONE) => {
                let input = self.input.take_input();
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    return;
                }
                if matches!(trimmed, "quit" | "exit" | "q") {
                    self.should_quit = true;
                    return;
                }
                let ih_id = self.save_input_history(trimmed);
                self.execute_command(trimmed, ih_id);
            }
            (KeyCode::Tab, KeyModifiers::NONE) => {
                let current = self.input.current_text();
                let results = self.completion.complete(&current, Some(&self.session.id));
                if results.len() == 1 {
                    self.input.complete_with(&results[0].value);
                }
            }
            (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                self.input.insert_char(c);
            }
            (KeyCode::Backspace, _) => {
                self.input.delete_char();
            }
            (KeyCode::Left, _) => {
                self.input.move_left();
            }
            (KeyCode::Right, _) => {
                self.input.move_right();
            }
            _ => {}
        }
    }

    fn drain_running(&mut self) {
        let mut finished = vec![];
        let mut got_output = false;
        for (ri, rc) in self.running_cmds.iter_mut().enumerate() {
            while let Ok(line) = rc.output_rx.try_recv() {
                got_output = true;
                if let Some(block) = self.blocks.get_mut(rc.block_idx) {
                    block.content.push_line(ShellOutputLine {
                        text: line,
                        stream: ShellOutputStream::Stdout,
                    });
                }
            }
            match rc.done_rx.try_recv() {
                Ok(exit_code) => {
                    if let Some(block) = self.blocks.get_mut(rc.block_idx) {
                        block.status = if exit_code == 0 {
                            BlockStatus::Success(exit_code)
                        } else {
                            BlockStatus::Failed(exit_code)
                        };
                    }
                    self.jobs.finish(rc.job_id, exit_code);
                    finished.push(ri);
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    if let Some(block) = self.blocks.get_mut(rc.block_idx) {
                        block.status = BlockStatus::Failed(-1);
                    }
                    self.jobs.finish(rc.job_id, -1);
                    finished.push(ri);
                }
                Err(oneshot::error::TryRecvError::Empty) => {}
            }
        }
        for ri in finished.into_iter().rev() {
            let rc = self.running_cmds.remove(ri);
            let (exit_code, lines) = self.blocks.get(rc.block_idx)
                .map(|b| {
                    let code = match b.status {
                        BlockStatus::Success(c) | BlockStatus::Failed(c) => c,
                        _ => -1,
                    };
                    (code, b.content.lines_ref().to_vec())
                }).unwrap_or((-1, vec![]));
            let output = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
            let duration = self.blocks.get(rc.block_idx)
                .map(|b| b.started_at.elapsed())
                .unwrap_or_default();
            tracing::info!(
                job_id = rc.job_id,
                exit_code = exit_code,
                output_lines = lines.len(),
                "command finished"
            );
            let mut cmd_output = crate::workflows::CommandOutput {
                lines,
                exit_code: Some(exit_code),
                duration,
            };
            let _ = self.pipeline.run_post_exec(&mut cmd_output, &self.session);
            if let Some(ih_id) = rc.input_history_id {
                let duration_ms = duration.as_millis() as i64;
                let preview = if output.len() > 500 { &output[..500] } else { &output };
                let db = self.db.lock().unwrap();
                if let Ok(ch_id) = db.save_command_result(
                    &self.session.id, &self.blocks.get(rc.block_idx).map(|b| b.command.as_str()).unwrap_or(""),
                    exit_code, duration_ms, preview,
                ) {
                    let _ = db.link_input_to_command(ih_id, ch_id);
                }
            }
            let event = ShellEvent::CommandFinished {
                job_id: rc.job_id,
                exit_code,
                output,
            };
            self.event_bus.emit(&event, &self.session);
        }
        if got_output {
            self.scroll_to_bottom();
        }

        let mut finished_chats = vec![];
        let mut pending_saves: Vec<(String, String)> = vec![];
        for (ci, rc) in self.running_chats.iter_mut().enumerate() {
            while let Ok(token) = rc.token_rx.try_recv() {
                got_output = true;
                if let Some(block) = self.blocks.get_mut(rc.block_idx) {
                    block.content.push_line(ShellOutputLine {
                        text: token,
                        stream: ShellOutputStream::Stdout,
                    });
                }
            }
            match rc.result_rx.try_recv() {
                Ok(Ok(result)) => {
                    tracing::info!(lines = result.block.content.line_count(), "chat response ok");
                    pending_saves.push((rc.user_message.clone(), result.response_text.clone()));
                    self.blocks[rc.block_idx] = result.block;
                    if rc.is_chat {
                        self.chat_history = result.updated_history;
                    }
                    finished_chats.push(ci);
                }
                Ok(Err(e)) => {
                    tracing::error!("chat error: {}", e);
                    if let Some(block) = self.blocks.get_mut(rc.block_idx) {
                        block.content = BlockContent::Plain(vec![ShellOutputLine {
                            text: format!("LLM error: {}", e),
                            stream: ShellOutputStream::Stderr,
                        }]);
                        block.status = BlockStatus::Failed(1);
                    }
                    finished_chats.push(ci);
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    if let Some(block) = self.blocks.get_mut(rc.block_idx) {
                        block.status = BlockStatus::Failed(-1);
                    }
                    finished_chats.push(ci);
                }
                Err(oneshot::error::TryRecvError::Empty) => {}
            }
        }
        for ci in finished_chats.into_iter().rev() {
            self.running_chats.remove(ci);
        }
        for (user_msg, assistant_msg) in pending_saves {
            self.save_chat_messages(&user_msg, &assistant_msg);
        }
        if got_output {
            self.scroll_to_bottom();
        }
    }

    fn scroll_to_bottom(&mut self) {
        self.block_scroll = u16::MAX;
    }

    fn scroll_to_focused(&mut self) {
        let Some(target_idx) = self.focused_block else { return };
        let viewport = self.blocks_viewport_h;
        if viewport == 0 { return; }
        let gap: u16 = 1;
        let max_block_h = (viewport / 2).max(3);
        let mut cumulative: u16 = 0;
        let mut target_top: u16 = 0;
        let mut target_h: u16 = 0;
        for (i, b) in self.blocks.iter().enumerate() {
            let h = if b.collapsed { 3 } else { ((b.content.line_count() as u16) + 2).min(max_block_h) };
            if i == target_idx {
                target_top = cumulative;
                target_h = h;
                break;
            }
            cumulative += h + if i + 1 < self.blocks.len() { gap } else { 0 };
        }
        let target_bottom = target_top.saturating_add(target_h);
        let view_bottom = self.block_scroll.saturating_add(viewport);
        if target_top < self.block_scroll {
            self.block_scroll = target_top;
        } else if target_bottom > view_bottom {
            self.block_scroll = target_bottom.saturating_sub(viewport);
        }
    }

    fn load_persisted_state(&mut self) {
        let db = self.db.lock().unwrap();
        if let Ok(history) = db.load_chat_history(&self.session.id) {
            self.chat_history = history.iter().map(|(role, content)| {
                match role.as_str() {
                    "user" => ChatMessage::user(content),
                    _ => ChatMessage::assistant_text(content),
                }
            }).collect();
            if !self.chat_history.is_empty() {
                tracing::info!(messages = self.chat_history.len(), "restored chat history");
            }
        }
        if let Ok(cmds) = db.load_input_history(&self.session.id) {
            if !cmds.is_empty() {
                tracing::info!(entries = cmds.len(), "restored input history");
                self.input.load_history(cmds);
            }
        }
    }

    fn save_chat_messages(&self, user_msg: &str, assistant_msg: &str) {
        let db = self.db.lock().unwrap();
        let _ = db.save_chat_message(&self.session.id, "user", user_msg);
        let _ = db.save_chat_message(&self.session.id, "assistant", assistant_msg);
    }

    fn save_input_history(&self, cmd: &str) -> Option<i64> {
        let db = self.db.lock().unwrap();
        db.save_input_history(&self.session.id, cmd).ok()
    }

    fn next_block_id(&mut self) -> usize {
        let id = self.next_block_id;
        self.next_block_id += 1;
        id
    }

    fn execute_command(&mut self, input: &str, input_history_id: Option<i64>) {
        tracing::info!(cmd = %input, "execute_command");
        match resolve::resolve(input) {
            ResolvedCommand::Builtin { name, .. } if name == "clear" => {
                self.blocks.retain(|b| matches!(b.status, BlockStatus::Running));
                self.focused_block = None;
                self.block_scroll = 0;
            }
            ResolvedCommand::Builtin { name, args } if name == "ask" || name == "chat" => {
                let is_chat = name == "chat";
                self.handle_chat_builtin(input, &args, is_chat);
            }
            ResolvedCommand::Builtin { name, args } if name == "sql" => {
                self.handle_sql_table(input, &args);
            }
            ResolvedCommand::Builtin { name, args } => {
                let lines = self.handle_builtin(&name, &args);
                let block_id = self.next_block_id();
                let mut block = Block {
                    id: block_id,
                    command: input.to_string(),
                    content: BlockContent::Plain(vec![]),
                    status: BlockStatus::Running,
                    collapsed: false,
                    started_at: std::time::Instant::now(),
                    job_id: None,
                    content_scroll: u16::MAX,
                };
                for line in lines {
                    block.content.push_line(ShellOutputLine {
                        text: line,
                        stream: ShellOutputStream::Stdout,
                    });
                }
                block.status = BlockStatus::Success(0);
                self.blocks.push(block);
                self.scroll_to_bottom();
            }
            ResolvedCommand::Shell { raw, background } => {
                let block_id = self.next_block_id();
                let mut block = Block {
                    id: block_id,
                    command: input.to_string(),
                    content: BlockContent::Ansi(vec![]),
                    status: BlockStatus::Running,
                    collapsed: false,
                    started_at: std::time::Instant::now(),
                    job_id: None,
                    content_scroll: u16::MAX,
                };

                if background {
                    let job_id = self.jobs.add(raw.clone(), block_id);
                    block.job_id = Some(job_id);

                    let env_vec: Vec<(String, String)> = self.session.env.iter()
                        .map(|(k, v)| (k.clone(), v.clone())).collect();
                    match crate::workflows::command::pty::PtyExecutor::spawn_background(&raw, env_vec) {
                        Ok(_handle) => {
                            block.content.push_line(ShellOutputLine {
                                text: format!("[{}] started in background", job_id),
                                stream: ShellOutputStream::Stdout,
                            });
                        }
                        Err(e) => {
                            block.content.push_line(ShellOutputLine {
                                text: format!("error spawning PTY: {}", e),
                                stream: ShellOutputStream::Stderr,
                            });
                            block.status = BlockStatus::Failed(-1);
                        }
                    }
                    self.blocks.push(block);
                    self.scroll_to_bottom();
                    let event = ShellEvent::CommandStarted { cmd: raw, job_id };
                    self.event_bus.emit(&event, &self.session);
                } else {
                    let job_id = self.jobs.add(raw.clone(), block_id);
                    block.job_id = Some(job_id);
                    self.blocks.push(block);
                    self.scroll_to_bottom();
                    let block_idx = self.blocks.len() - 1;

                    let started_event = ShellEvent::CommandStarted { cmd: raw.clone(), job_id };
                    self.event_bus.emit(&started_event, &self.session);

                    let mut shell_cmd = crate::workflows::ShellCommand {
                        raw: raw.clone(),
                        program: "sh".into(),
                        args: vec!["-c".into(), raw.clone()],
                        env_overrides: self.session.env.clone(),
                        working_dir: self.session.working_dir.clone(),
                    };
                    let _ = self.pipeline.run_pre_exec(&mut shell_cmd, &self.session);

                    let env_vec: Vec<(String, String)> = self.session.env.iter()
                        .map(|(k, v)| (k.clone(), v.clone())).collect();

                    match crate::workflows::command::pty::PtyExecutor::spawn_streaming(
                        &shell_cmd.raw, env_vec,
                    ) {
                        Ok((output_rx, done_rx, kill_handle)) => {
                            self.running_cmds.push(RunningCommand {
                                block_idx,
                                job_id,
                                input_history_id,
                                output_rx,
                                done_rx,
                                kill_handle,
                            });
                        }
                        Err(e) => {
                            self.blocks[block_idx].content.push_line(ShellOutputLine {
                                text: format!("error: {e}"),
                                stream: ShellOutputStream::Stderr,
                            });
                            self.blocks[block_idx].status = BlockStatus::Failed(-1);
                            self.jobs.finish(job_id, -1);
                            let finished_event = ShellEvent::CommandFinished { job_id, exit_code: -1, output: format!("error: {e}") };
                            self.event_bus.emit(&finished_event, &self.session);
                        }
                    }
                }
            }
        }
    }

    fn handle_chat_builtin(&mut self, input: &str, args: &[String], is_chat: bool) {
        let message = args.join(" ");
        if message.is_empty() {
            let block_id = self.next_block_id();
            self.blocks.push(Block {
                id: block_id,
                command: input.to_string(),
                content: BlockContent::Plain(vec![ShellOutputLine {
                    text: "Usage: ask <question>".into(),
                    stream: ShellOutputStream::Stderr,
                }]),
                status: BlockStatus::Failed(1),
                collapsed: false,
                started_at: std::time::Instant::now(),
                job_id: None,
                content_scroll: u16::MAX,
            });
            self.scroll_to_bottom();
            return;
        }
        let (provider, tools) = match (&self.provider, &self.tools) {
            (Some(p), Some(t)) => (p.clone(), t.clone()),
            _ => {
                let block_id = self.next_block_id();
                self.blocks.push(Block {
                    id: block_id,
                    command: input.to_string(),
                    content: BlockContent::Plain(vec![ShellOutputLine {
                        text: "No LLM provider configured. Start with: redtrail shell --llm anthropic".into(),
                        stream: ShellOutputStream::Stderr,
                    }]),
                    status: BlockStatus::Failed(1),
                    collapsed: false,
                    started_at: std::time::Instant::now(),
                    job_id: None,
                    content_scroll: u16::MAX,
                });
                self.scroll_to_bottom();
                return;
            }
        };

        tracing::info!(history_len = self.chat_history.len(), "chat request: {}", message);

        let block_id = self.next_block_id();
        self.blocks.push(Block {
            id: block_id,
            command: input.to_string(),
            content: BlockContent::Plain(vec![]),
            status: BlockStatus::Running,
            collapsed: false,
            started_at: std::time::Instant::now(),
            job_id: None,
            content_scroll: u16::MAX,
        });
        self.scroll_to_bottom();
        let block_idx = self.blocks.len() - 1;

        let (token_tx, token_rx) = mpsc::unbounded_channel();
        let (result_tx, result_rx) = oneshot::channel();

        let workflow = ChatWorkflow {
            provider,
            tools,
        };
        let user_msg = message.clone();
        let recent = {
            let db = self.db.lock().unwrap();
            db.load_recent_commands(&self.session.id, 20)
                .unwrap_or_default()
                .into_iter()
                .map(|(cmd, ch_id)| RecentCommand { command: cmd, command_history_id: ch_id })
                .collect()
        };
        let history = if is_chat { self.chat_history.clone() } else { vec![] };
        let chat_input = ChatInput {
            user_message: message,
            history,
            recent_commands: recent,
        };

        tokio::spawn(async move {
            let result = workflow.execute_streaming(chat_input, token_tx).await;
            let _ = result_tx.send(result);
        });

        self.running_chats.push(RunningChat {
            block_idx,
            user_message: user_msg,
            is_chat,
            token_rx,
            result_rx,
        });
    }

    fn handle_builtin(&mut self, name: &str, args: &[String]) -> Vec<String> {
        match name {
            "session" => self.handle_session_builtin(args),
            "jobs" => self.handle_jobs_builtin(),
            "help" => vec![
                "builtins: session, sql, jobs, ask, chat, clear, help".into(),
                "  session list|info".into(),
                "  sql <query>".into(),
                "  ask <question> — one-shot LLM query".into(),
                "  chat <message> — conversational LLM".into(),
                "  jobs".into(),
                "  clear — remove finished blocks".into(),
            ],
            _ => vec![format!("{name}: not yet implemented")],
        }
    }

    fn handle_session_builtin(&mut self, args: &[String]) -> Vec<String> {
        let sub = args.first().map(|s| s.as_str()).unwrap_or("list");
        match sub {
            "info" => {
                vec![
                    format!("name: {}", self.session.name),
                    format!("id: {}", self.session.id),
                    format!("target hosts: {}", self.session.target.hosts.len()),
                    format!("provider: {} / {}", self.session.llm_provider, self.session.llm_model),
                    format!("working_dir: {}", self.session.working_dir.display()),
                    format!("prompt_template: {}", self.session.prompt_template),
                    format!("env vars: {}", self.session.env.len()),
                ]
            }
            _ => {
                let db = self.db.lock().unwrap();
                match SessionWorkflow::list(&db) {
                    Ok(sessions) => {
                        let mut lines = vec![format!("{:<36}  {:<20}  STATUS", "ID", "NAME")];
                        for s in sessions {
                            let marker = if s.id == self.session.id { "*" } else { " " };
                            lines.push(format!("{} {:<36}  {:<20}", marker, s.id, s.name));
                        }
                        lines
                    }
                    Err(e) => vec![format!("error: {e}")],
                }
            }
        }
    }

    fn handle_sql_table(&mut self, input: &str, args: &[String]) {
        let raw = args.join(" ");
        let block_id = self.next_block_id();
        if raw.trim().is_empty() {
            self.blocks.push(Block {
                id: block_id,
                command: input.to_string(),
                content: BlockContent::Plain(vec![ShellOutputLine {
                    text: "usage: sql <query>".into(),
                    stream: ShellOutputStream::Stderr,
                }]),
                status: BlockStatus::Failed(1),
                collapsed: false,
                started_at: std::time::Instant::now(),
                job_id: None,
                content_scroll: u16::MAX,
            });
            self.scroll_to_bottom();
            return;
        }
        let result = {
            let db = self.db.lock().unwrap();
            QueryWorkflow::new().run_raw(&db, QueryInput { raw })
        };
        match result {
            Ok((headers, rows)) => {
                self.blocks.push(Block {
                    id: block_id,
                    command: input.to_string(),
                    content: BlockContent::Table(crate::workflows::TableData { headers, rows }),
                    status: BlockStatus::Success(0),
                    collapsed: false,
                    started_at: std::time::Instant::now(),
                    job_id: None,
                    content_scroll: u16::MAX,
                });
            }
            Err(e) => {
                self.blocks.push(Block {
                    id: block_id,
                    command: input.to_string(),
                    content: BlockContent::Plain(vec![ShellOutputLine {
                        text: format!("error: {e}"),
                        stream: ShellOutputStream::Stderr,
                    }]),
                    status: BlockStatus::Failed(1),
                    collapsed: false,
                    started_at: std::time::Instant::now(),
                    job_id: None,
                    content_scroll: u16::MAX,
                });
            }
        }
        self.scroll_to_bottom();
    }

    fn handle_jobs_builtin(&self) -> Vec<String> {
        let jobs = self.jobs.list();
        if jobs.is_empty() {
            return vec!["no jobs".into()];
        }
        let mut lines = vec![format!("{:<4}  {:<40}  STATUS", "ID", "CMD")];
        for job in jobs {
            let status = if job.finished {
                format!("done({})", job.exit_code.unwrap_or(0))
            } else {
                "running".into()
            };
            lines.push(format!("{:<4}  {:<40}  {}", job.id, job.command, status));
        }
        lines
    }

    fn build_prompt_vars(&self) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert("session".into(), self.session.name.clone());
        vars.insert(
            "target".into(),
            self.session.target.hosts.first().cloned().unwrap_or_default(),
        );
        vars.insert(
            "cwd".into(),
            self.session.working_dir.to_string_lossy().into_owned(),
        );
        vars.insert("jobs".into(), self.jobs.running_count().to_string());
        vars.insert(
            "status".into(),
            if self.jobs.running_count() > 0 { "busy" } else { "ready" }.into(),
        );
        vars
    }
}
