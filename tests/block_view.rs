use redtrail::workflows::{Block, BlockContent, BlockStatus, ShellOutputLine, ShellOutputStream};
use std::time::Instant;

#[test]
fn block_summary_when_collapsed() {
    let block = Block {
        id: 1,
        command: "nmap -sV 10.10.10.1".into(),
        content: BlockContent::Ansi(vec![
            ShellOutputLine { text: "line1".into(), stream: ShellOutputStream::Stdout },
            ShellOutputLine { text: "line2".into(), stream: ShellOutputStream::Stdout },
        ]),
        status: BlockStatus::Success(0),
        collapsed: true,
        started_at: Instant::now(),
        job_id: None,
        content_scroll: 0,
    };
    assert_eq!(block.content.line_count(), 2);
    assert!(block.collapsed);
    assert!(matches!(block.status, BlockStatus::Success(0)));
}

#[test]
fn block_tracks_background_job() {
    let block = Block {
        id: 2,
        command: "gobuster dir -u http://target &".into(),
        content: BlockContent::Ansi(vec![]),
        status: BlockStatus::Running,
        collapsed: false,
        started_at: Instant::now(),
        job_id: Some(1),
        content_scroll: 0,
    };
    assert!(block.job_id.is_some());
    assert!(matches!(block.status, BlockStatus::Running));
}
