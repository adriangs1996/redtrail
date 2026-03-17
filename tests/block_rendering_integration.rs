use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use redtrail::{Block, BlockContent, BlockStatus, ShellOutputLine, ShellOutputStream, TableData};
use redtrail::tui::widgets::block_view::BlockView;

fn make_block(content: BlockContent) -> Block {
    Block {
        id: 0,
        command: "test".into(),
        content,
        status: BlockStatus::Success(0),
        collapsed: false,
        started_at: std::time::Instant::now(),
        job_id: None,
        content_scroll: 0,
    }
}

#[test]
fn render_plain_block() {
    let block = make_block(BlockContent::Plain(vec![
        ShellOutputLine { text: "hello".into(), stream: ShellOutputStream::Stdout },
    ]));
    let area = Rect::new(0, 0, 40, 5);
    let mut buf = Buffer::empty(area);
    BlockView::new(&block, false).render(area, &mut buf);
    let mut content = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            content.push_str(buf[(x, y)].symbol());
        }
    }
    assert!(content.contains("hello"));
}

#[test]
fn render_ansi_block() {
    let block = make_block(BlockContent::Ansi(vec![
        ShellOutputLine { text: "\x1b[32mgreen\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ]));
    let area = Rect::new(0, 0, 40, 5);
    let mut buf = Buffer::empty(area);
    BlockView::new(&block, false).render(area, &mut buf);
    let mut content = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            content.push_str(buf[(x, y)].symbol());
        }
    }
    assert!(content.contains("green"));
}

#[test]
fn render_markdown_block() {
    let block = make_block(BlockContent::Markdown(vec![
        ShellOutputLine { text: "# Title".into(), stream: ShellOutputStream::Stdout },
    ]));
    let area = Rect::new(0, 0, 40, 5);
    let mut buf = Buffer::empty(area);
    BlockView::new(&block, false).render(area, &mut buf);
    let mut content = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            content.push_str(buf[(x, y)].symbol());
        }
    }
    assert!(content.contains("Title"));
}

#[test]
fn render_table_block() {
    let block = make_block(BlockContent::Table(TableData {
        headers: vec!["id".into()],
        rows: vec![vec!["1".into()]],
    }));
    let area = Rect::new(0, 0, 40, 10);
    let mut buf = Buffer::empty(area);
    BlockView::new(&block, false).render(area, &mut buf);
    let top: String = (0..buf.area.width).map(|x| buf[(x, 1)].symbol().to_string()).collect();
    assert!(top.contains('╭'));
}

#[test]
fn render_collapsed_shows_line_count() {
    let mut block = make_block(BlockContent::Ansi(vec![
        ShellOutputLine { text: "line1".into(), stream: ShellOutputStream::Stdout },
        ShellOutputLine { text: "line2".into(), stream: ShellOutputStream::Stdout },
    ]));
    block.collapsed = true;
    let area = Rect::new(0, 0, 40, 5);
    let mut buf = Buffer::empty(area);
    BlockView::new(&block, false).render(area, &mut buf);
    let mut content = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            content.push_str(buf[(x, y)].symbol());
        }
    }
    assert!(content.contains("2 lines"));
}
