use ratatui::prelude::*;
use ratatui::symbols::border;
use ratatui::widgets::{Block as RatatuiBlock, Borders, Paragraph, Wrap};
use crate::workflows::{Block, BlockContent, BlockStatus};

pub struct BlockView<'a> {
    block: &'a Block,
    focused: bool,
    content_scroll: u16,
}

impl<'a> BlockView<'a> {
    pub fn new(block: &'a Block, focused: bool) -> Self {
        Self { block, focused, content_scroll: block.content_scroll }
    }

    fn effective_scroll(&self, inner_height: u16) -> u16 {
        let total_lines = self.block.content.line_count() as u16;
        let max_scroll = total_lines.saturating_sub(inner_height);
        if self.content_scroll == u16::MAX {
            max_scroll
        } else {
            self.content_scroll.min(max_scroll)
        }
    }

    fn title_spans(&self) -> Vec<Span<'a>> {
        let status_icon = match self.block.status {
            BlockStatus::Running => Span::styled(" ⟳ ", Style::default().fg(Color::Yellow)),
            BlockStatus::Success(_) => Span::styled(" ✓ ", Style::default().fg(Color::Green)),
            BlockStatus::Failed(_) => Span::styled(" ✗ ", Style::default().fg(Color::Red)),
        };

        let cmd = Span::styled(
            self.block.command.clone(),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        );

        let elapsed = self.block.started_at.elapsed();
        let elapsed_str = if elapsed.as_secs() >= 60 {
            format!(" {}m{}s ", elapsed.as_secs() / 60, elapsed.as_secs() % 60)
        } else {
            format!(" {}s ", elapsed.as_secs())
        };
        let time = Span::styled(elapsed_str, Style::default().fg(Color::DarkGray));

        vec![status_icon, cmd, time]
    }

    fn border_color(&self) -> Color {
        if self.focused {
            return Color::Cyan;
        }
        match self.block.status {
            BlockStatus::Running => Color::Yellow,
            BlockStatus::Success(_) => Color::DarkGray,
            BlockStatus::Failed(_) => Color::Red,
        }
    }
}

impl<'a> Widget for BlockView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_color = self.border_color();
        let title = Line::from(self.title_spans());
        let container = RatatuiBlock::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(border_color))
            .title(title);

        let inner = container.inner(area);
        container.render(area, buf);

        if inner.height == 0 { return; }

        if self.block.collapsed {
            let summary = format!("({} lines, exit {})",
                self.block.content.line_count(),
                match self.block.status {
                    BlockStatus::Success(c) | BlockStatus::Failed(c) => c.to_string(),
                    BlockStatus::Running => "…".to_string(),
                },
            );
            Paragraph::new(Span::styled(summary, Style::default().fg(Color::DarkGray)))
                .render(inner, buf);
        } else {
            let render_lines = match &self.block.content {
                BlockContent::Plain(lines) => {
                    super::renderers::plain::render(lines)
                }
                BlockContent::Markdown(lines) => {
                    super::renderers::markdown::render(lines)
                }
                BlockContent::Ansi(lines) => {
                    super::renderers::ansi::render(lines)
                }
                BlockContent::Table(data) => super::renderers::table::render(data, inner.width),
            };
            let scroll_y = self.effective_scroll(inner.height);
            Paragraph::new(render_lines)
                .wrap(Wrap { trim: false })
                .scroll((scroll_y, 0))
                .render(inner, buf);
        }
    }
}
