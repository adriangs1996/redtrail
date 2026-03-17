use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::channel::InputMode;

pub struct InputBar {
    pub input: String,
    pub cursor: usize,
    prompt: String,
    history: Vec<String>,
    history_idx: Option<usize>,
    saved_input: String,
}

impl Default for InputBar {
    fn default() -> Self {
        Self {
            input: String::new(),
            cursor: 0,
            prompt: "$ ".to_string(),
            history: Vec::new(),
            history_idx: None,
            saved_input: String::new(),
        }
    }
}

impl InputBar {
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn delete_char(&mut self) {
        if self.cursor > 0 {
            let prev = self.input[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.remove(prev);
            self.cursor = prev;
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.input[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor = self.input[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.input.len());
        }
    }

    pub fn take_input(&mut self) -> String {
        let text = self.input.clone();
        if !text.trim().is_empty() {
            self.history.push(text.clone());
        }
        self.input.clear();
        self.cursor = 0;
        self.history_idx = None;
        self.saved_input.clear();
        text
    }

    pub fn load_history(&mut self, entries: Vec<String>) {
        self.history = entries;
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() { return; }
        match self.history_idx {
            None => {
                self.saved_input = self.input.clone();
                self.history_idx = Some(self.history.len() - 1);
            }
            Some(0) => return,
            Some(idx) => {
                self.history_idx = Some(idx - 1);
            }
        }
        if let Some(idx) = self.history_idx {
            self.input = self.history[idx].clone();
            self.cursor = self.input.len();
        }
    }

    pub fn history_down(&mut self) {
        let Some(idx) = self.history_idx else { return };
        if idx + 1 < self.history.len() {
            self.history_idx = Some(idx + 1);
            self.input = self.history[idx + 1].clone();
            self.cursor = self.input.len();
        } else {
            self.history_idx = None;
            self.input = self.saved_input.clone();
            self.cursor = self.input.len();
        }
    }

    pub fn set_prompt(&mut self, prompt: &str) {
        self.prompt = prompt.to_string();
    }

    pub fn current_text(&self) -> String {
        self.input.clone()
    }

    pub fn complete_with(&mut self, value: &str) {
        let before = &self.input[..self.cursor];
        let last_word_start = before.rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        self.input = format!("{}{}{}", &self.input[..last_word_start], value, &self.input[self.cursor..]);
        self.cursor = last_word_start + value.len();
    }

    pub fn render(&self, f: &mut Frame, area: Rect, mode: InputMode) {
        let (title, prompt_color) = match mode {
            InputMode::Chat => (" chat ", Color::Cyan),
            InputMode::Terminal => (" terminal ", Color::Green),
        };
        let prompt = &self.prompt;

        let before_cursor = &self.input[..self.cursor];
        let char_len = self.input[self.cursor..].chars().next().map_or(0, |c| c.len_utf8());
        let at_cursor = if char_len > 0 {
            &self.input[self.cursor..self.cursor + char_len]
        } else {
            " "
        };
        let after_cursor = if char_len > 0 {
            &self.input[self.cursor + char_len..]
        } else {
            ""
        };

        let line = Line::from(vec![
            Span::styled(
                prompt,
                Style::default()
                    .fg(prompt_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(before_cursor.to_string()),
            Span::styled(
                at_cursor.to_string(),
                Style::default().fg(Color::Black).bg(Color::White),
            ),
            Span::raw(after_cursor.to_string()),
        ]);

        let widget =
            Paragraph::new(line).block(Block::default().borders(Borders::ALL).title(title));
        f.render_widget(widget, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_delete() {
        let mut bar = InputBar::default();
        bar.insert_char('h');
        bar.insert_char('i');
        assert_eq!(bar.input, "hi");
        assert_eq!(bar.cursor, 2);
        bar.delete_char();
        assert_eq!(bar.input, "h");
        assert_eq!(bar.cursor, 1);
    }

    #[test]
    fn test_cursor_movement() {
        let mut bar = InputBar::default();
        bar.insert_char('a');
        bar.insert_char('b');
        bar.insert_char('c');
        bar.move_left();
        assert_eq!(bar.cursor, 2);
        bar.move_left();
        assert_eq!(bar.cursor, 1);
        bar.move_right();
        assert_eq!(bar.cursor, 2);
    }

    #[test]
    fn test_take_input() {
        let mut bar = InputBar::default();
        bar.insert_char('x');
        bar.insert_char('y');
        let text = bar.take_input();
        assert_eq!(text, "xy");
        assert!(bar.input.is_empty());
        assert_eq!(bar.cursor, 0);
    }

    #[test]
    fn test_move_boundaries() {
        let mut bar = InputBar::default();
        bar.move_left();
        assert_eq!(bar.cursor, 0);
        bar.move_right();
        assert_eq!(bar.cursor, 0);
    }
}
