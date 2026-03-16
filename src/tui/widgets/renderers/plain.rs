use ratatui::prelude::*;
use crate::workflows::{ShellOutputLine, ShellOutputStream};

pub fn render(lines: &[ShellOutputLine]) -> Vec<Line<'_>> {
    lines.iter().map(|l| {
        let color = match l.stream {
            ShellOutputStream::Stdout => Color::White,
            ShellOutputStream::Stderr => Color::Red,
        };
        Line::from(Span::styled(l.text.as_str(), Style::default().fg(color)))
    }).collect()
}
