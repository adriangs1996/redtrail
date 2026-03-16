use redtrail::{BlockContent, ShellOutputLine, ShellOutputStream, TableData};

#[test]
fn plain_line_count() {
    let content = BlockContent::Plain(vec![
        ShellOutputLine { text: "a".into(), stream: ShellOutputStream::Stdout },
        ShellOutputLine { text: "b".into(), stream: ShellOutputStream::Stdout },
    ]);
    assert_eq!(content.line_count(), 2);
}

#[test]
fn ansi_line_count() {
    let content = BlockContent::Ansi(vec![
        ShellOutputLine { text: "\x1b[31mred\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ]);
    assert_eq!(content.line_count(), 1);
}

#[test]
fn markdown_line_count() {
    let content = BlockContent::Markdown(vec![
        ShellOutputLine { text: "# Header".into(), stream: ShellOutputStream::Stdout },
        ShellOutputLine { text: "body".into(), stream: ShellOutputStream::Stdout },
    ]);
    assert_eq!(content.line_count(), 2);
}

#[test]
fn table_line_count() {
    let data = TableData {
        headers: vec!["id".into(), "name".into()],
        rows: vec![
            vec!["1".into(), "test".into()],
            vec!["2".into(), "prod".into()],
        ],
    };
    let content = BlockContent::Table(data);
    assert_eq!(content.line_count(), 6);
}

#[test]
fn table_empty_line_count() {
    let data = TableData {
        headers: vec!["id".into()],
        rows: vec![],
    };
    let content = BlockContent::Table(data);
    assert_eq!(content.line_count(), 4);
}

#[test]
fn push_line_on_plain() {
    let mut content = BlockContent::Plain(vec![]);
    content.push_line(ShellOutputLine { text: "hello".into(), stream: ShellOutputStream::Stdout });
    assert_eq!(content.line_count(), 1);
}

#[test]
fn push_line_on_table_is_noop() {
    let mut content = BlockContent::Table(TableData { headers: vec![], rows: vec![] });
    content.push_line(ShellOutputLine { text: "hello".into(), stream: ShellOutputStream::Stdout });
    assert_eq!(content.line_count(), 4);
}

#[test]
fn push_token_on_markdown() {
    let mut content = BlockContent::Markdown(vec![]);
    content.push_token("hello ");
    content.push_token("world");
    assert_eq!(content.line_count(), 1);
    if let BlockContent::Markdown(lines) = &content {
        assert_eq!(lines[0].text, "hello world");
    }
}

#[test]
fn push_token_newline_splits() {
    let mut content = BlockContent::Markdown(vec![]);
    content.push_token("line1\nline2");
    assert_eq!(content.line_count(), 2);
}

#[test]
fn lines_ref_returns_lines() {
    let content = BlockContent::Plain(vec![
        ShellOutputLine { text: "a".into(), stream: ShellOutputStream::Stdout },
    ]);
    assert_eq!(content.lines_ref().len(), 1);
}

#[test]
fn lines_ref_on_table_is_empty() {
    let content = BlockContent::Table(TableData { headers: vec![], rows: vec![] });
    assert!(content.lines_ref().is_empty());
}
