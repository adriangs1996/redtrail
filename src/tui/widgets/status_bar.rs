use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub struct StatusBarData {
    pub session_name: String,
    pub running_jobs: usize,
    pub host_count: usize,
    pub cred_count: usize,
    pub flag_count: usize,
}

pub struct StatusBar<'a> {
    data: &'a StatusBarData,
}

impl<'a> StatusBar<'a> {
    pub fn new(data: &'a StatusBarData) -> Self {
        Self { data }
    }
}

impl<'a> Widget for StatusBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let spans = vec![
            Span::styled(
                format!(" [session: {}] ", self.data.session_name),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("[{} jobs] ", self.data.running_jobs),
                Style::default().fg(if self.data.running_jobs > 0 { Color::Yellow } else { Color::DarkGray }),
            ),
            Span::styled(
                format!("[{} hosts | {} creds | {} flags]",
                    self.data.host_count, self.data.cred_count, self.data.flag_count),
                Style::default().fg(Color::Green),
            ),
        ];

        Paragraph::new(Line::from(spans))
            .style(Style::default().bg(Color::Rgb(30, 30, 50)))
            .render(area, buf);
    }
}
