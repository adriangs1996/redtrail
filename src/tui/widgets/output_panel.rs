use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

#[derive(Clone, PartialEq)]
pub enum LineKind {
    UserInput,
    Response,
    Error,
}

#[derive(Clone)]
pub struct OutputLine {
    pub text: String,
    pub kind: LineKind,
}

pub struct OutputPanel {
    pub lines: Vec<OutputLine>,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub processing: bool,
}

impl Default for OutputPanel {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            scroll_offset: 0,
            auto_scroll: true,
            processing: false,
        }
    }
}

impl OutputPanel {
    pub fn push_line(&mut self, text: String, kind: LineKind) {
        self.lines.push(OutputLine { text, kind });
    }

    pub fn append_to_last(&mut self, text: &str) {
        if let Some(last) = self.lines.last_mut()
            && last.kind == LineKind::Response
        {
            last.text.push_str(text);
            return;
        }
        self.push_line(text.to_string(), LineKind::Response);
    }

    pub fn append_token(&mut self, text: String) {
        if text.contains('\n') {
            for (i, chunk) in text.split('\n').enumerate() {
                if i == 0 {
                    self.append_to_last(chunk);
                } else {
                    self.push_line(chunk.to_string(), LineKind::Response);
                }
            }
        } else {
            self.append_to_last(&text);
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
        self.auto_scroll = false;
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset += 1;
        self.auto_scroll = false;
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let visible_height = area.height.saturating_sub(2) as usize;
        let total = self.lines.len();

        if self.auto_scroll && total > visible_height {
            self.scroll_offset = total.saturating_sub(visible_height);
        }
        if self.scroll_offset > total.saturating_sub(visible_height.min(total)) {
            self.scroll_offset = total.saturating_sub(visible_height.min(total));
        }

        let lines: Vec<Line> = self
            .lines
            .iter()
            .skip(self.scroll_offset)
            .take(visible_height)
            .map(|ol| {
                let color = match ol.kind {
                    LineKind::UserInput => Color::Cyan,
                    LineKind::Response => Color::White,
                    LineKind::Error => Color::Red,
                };
                Line::from(Span::styled(&ol.text, Style::default().fg(color)))
            })
            .collect();

        let status = if self.processing { " ⟳ " } else { "" };
        let widget = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" redtrail{status} ")),
            )
            .wrap(Wrap { trim: false });
        f.render_widget(widget, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_line() {
        let mut panel = OutputPanel::default();
        panel.push_line("hello".into(), LineKind::Response);
        assert_eq!(panel.lines.len(), 1);
        assert_eq!(panel.lines[0].text, "hello");
    }

    #[test]
    fn test_append_token_simple() {
        let mut panel = OutputPanel::default();
        panel.append_token("hello ".into());
        panel.append_token("world".into());
        assert_eq!(panel.lines.len(), 1);
        assert_eq!(panel.lines[0].text, "hello world");
    }

    #[test]
    fn test_append_token_with_newlines() {
        let mut panel = OutputPanel::default();
        panel.append_token("line1\nline2\nline3".into());
        assert_eq!(panel.lines.len(), 3);
        assert_eq!(panel.lines[0].text, "line1");
        assert_eq!(panel.lines[1].text, "line2");
        assert_eq!(panel.lines[2].text, "line3");
    }

    #[test]
    fn test_append_does_not_merge_into_user_input() {
        let mut panel = OutputPanel::default();
        panel.push_line("> cmd".into(), LineKind::UserInput);
        panel.append_token("response".into());
        assert_eq!(panel.lines.len(), 2);
    }

    #[test]
    fn test_scroll() {
        let mut panel = OutputPanel::default();
        panel.scroll_up();
        assert_eq!(panel.scroll_offset, 0);
        assert!(!panel.auto_scroll);

        panel.auto_scroll = true;
        panel.scroll_down();
        assert_eq!(panel.scroll_offset, 1);
        assert!(!panel.auto_scroll);
    }
}
