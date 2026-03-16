use ratatui::style::Color;
use redtrail::{ShellOutputLine, ShellOutputStream};

#[test]
fn plain_stdout_white() {
    let lines = vec![
        ShellOutputLine { text: "hello".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::plain::render(&lines);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].spans.len(), 1);
    assert_eq!(result[0].spans[0].content, "hello");
    assert_eq!(result[0].spans[0].style.fg, Some(Color::White));
}

#[test]
fn plain_stderr_red() {
    let lines = vec![
        ShellOutputLine { text: "error".into(), stream: ShellOutputStream::Stderr },
    ];
    let result = redtrail::tui::widgets::renderers::plain::render(&lines);
    assert_eq!(result[0].spans[0].style.fg, Some(Color::Red));
}

#[test]
fn plain_mixed_streams() {
    let lines = vec![
        ShellOutputLine { text: "ok".into(), stream: ShellOutputStream::Stdout },
        ShellOutputLine { text: "err".into(), stream: ShellOutputStream::Stderr },
        ShellOutputLine { text: "ok2".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::plain::render(&lines);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].spans[0].style.fg, Some(Color::White));
    assert_eq!(result[1].spans[0].style.fg, Some(Color::Red));
    assert_eq!(result[2].spans[0].style.fg, Some(Color::White));
}

#[test]
fn plain_empty() {
    let lines: Vec<ShellOutputLine> = vec![];
    let result = redtrail::tui::widgets::renderers::plain::render(&lines);
    assert!(result.is_empty());
}
