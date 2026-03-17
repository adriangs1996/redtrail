use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
use ratatui::prelude::Widget;
use std::time::Instant;
use redtrail::workflows::{Block, BlockContent, BlockStatus, ShellOutputLine, ShellOutputStream};
use redtrail::tui::widgets::block_view::BlockView;
use redtrail::tui::widgets::status_bar::{StatusBar, StatusBarData};

#[test]
fn block_view_renders_command_header() {
    let backend = TestBackend::new(60, 10);
    let mut terminal = Terminal::new(backend).unwrap();

    let block = Block {
        id: 1,
        command: "nmap -sV 10.10.10.1".into(),
        content: BlockContent::Ansi(vec![
            ShellOutputLine { text: "PORT   STATE SERVICE".into(), stream: ShellOutputStream::Stdout },
            ShellOutputLine { text: "22/tcp open  ssh".into(), stream: ShellOutputStream::Stdout },
        ]),
        status: BlockStatus::Success(0),
        collapsed: false,
        started_at: Instant::now(),
        job_id: None,
        content_scroll: 0,
    };

    terminal.draw(|f| {
        let area = f.area();
        BlockView::new(&block, false).render(area, f.buffer_mut());
    }).unwrap();

    let buf = terminal.backend().buffer().clone();
    let content = buffer_to_string(&buf);
    assert!(content.contains("nmap -sV 10.10.10.1"), "header should contain command");
    assert!(content.contains("22/tcp"), "output should contain port line");
}

#[test]
fn block_view_collapsed_shows_summary() {
    let backend = TestBackend::new(60, 5);
    let mut terminal = Terminal::new(backend).unwrap();

    let block = Block {
        id: 1,
        command: "nmap scan".into(),
        content: BlockContent::Ansi(vec![
            ShellOutputLine { text: "line1".into(), stream: ShellOutputStream::Stdout },
            ShellOutputLine { text: "line2".into(), stream: ShellOutputStream::Stdout },
            ShellOutputLine { text: "line3".into(), stream: ShellOutputStream::Stdout },
        ]),
        status: BlockStatus::Success(0),
        collapsed: true,
        started_at: Instant::now(),
        job_id: None,
        content_scroll: 0,
    };

    terminal.draw(|f| {
        let area = f.area();
        BlockView::new(&block, false).render(area, f.buffer_mut());
    }).unwrap();

    let buf = terminal.backend().buffer().clone();
    let content = buffer_to_string(&buf);
    assert!(content.contains("3 lines"), "collapsed should show line count");
    assert!(!content.contains("line1"), "collapsed should not show output");
}

#[test]
fn block_view_running_shows_spinner() {
    let backend = TestBackend::new(60, 5);
    let mut terminal = Terminal::new(backend).unwrap();

    let block = Block {
        id: 1,
        command: "long running".into(),
        content: BlockContent::Ansi(vec![]),
        status: BlockStatus::Running,
        collapsed: false,
        started_at: Instant::now(),
        job_id: Some(1),
        content_scroll: 0,
    };

    terminal.draw(|f| {
        let area = f.area();
        BlockView::new(&block, false).render(area, f.buffer_mut());
    }).unwrap();

    let buf = terminal.backend().buffer().clone();
    let content = buffer_to_string(&buf);
    assert!(content.contains("⟳") || content.contains("▶"), "running block should show indicator");
}

#[test]
fn block_view_failed_shows_x() {
    let backend = TestBackend::new(60, 5);
    let mut terminal = Terminal::new(backend).unwrap();

    let block = Block {
        id: 1,
        command: "failing cmd".into(),
        content: BlockContent::Plain(vec![
            ShellOutputLine { text: "error occurred".into(), stream: ShellOutputStream::Stderr },
        ]),
        status: BlockStatus::Failed(1),
        collapsed: false,
        started_at: Instant::now(),
        job_id: None,
        content_scroll: 0,
    };

    terminal.draw(|f| {
        let area = f.area();
        BlockView::new(&block, false).render(area, f.buffer_mut());
    }).unwrap();

    let buf = terminal.backend().buffer().clone();
    let content = buffer_to_string(&buf);
    assert!(content.contains("✗"), "failed block should show ✗");
}

#[test]
fn status_bar_renders_session_info() {
    let backend = TestBackend::new(80, 1);
    let mut terminal = Terminal::new(backend).unwrap();

    let data = StatusBarData {
        session_name: "htb-machine".into(),
        running_jobs: 2,
        host_count: 5,
        cred_count: 3,
        flag_count: 1,
    };

    terminal.draw(|f| {
        let area = f.area();
        StatusBar::new(&data).render(area, f.buffer_mut());
    }).unwrap();

    let buf = terminal.backend().buffer().clone();
    let content = buffer_to_string(&buf);
    assert!(content.contains("htb-machine"), "should show session name");
    assert!(content.contains("[2 jobs]"), "should show job count");
    assert!(content.contains("5 hosts"), "should show host count");
}

fn buffer_to_string(buf: &Buffer) -> String {
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let cell = buf.cell((x, y)).unwrap();
            s.push_str(cell.symbol());
        }
        s.push('\n');
    }
    s
}
