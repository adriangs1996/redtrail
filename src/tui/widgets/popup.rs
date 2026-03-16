use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState};
use crossterm::event::{KeyCode, KeyEvent};

pub struct PopupItem {
    pub label: String,
    pub detail: String,
    pub enabled: bool,
}

pub struct PopupState {
    pub items: Vec<PopupItem>,
    pub list_state: ListState,
    pub visible: bool,
    pub title: String,
    pub confirmed: bool,
}

impl PopupState {
    pub fn new(title: String, items: Vec<PopupItem>) -> Self {
        let mut list_state = ListState::default();
        if !items.is_empty() {
            list_state.select(Some(0));
        }
        Self { items, list_state, visible: true, title, confirmed: false }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> PopupAction {
        match key.code {
            KeyCode::Up => {
                let i = self.list_state.selected().unwrap_or(0);
                if i > 0 {
                    self.list_state.select(Some(i - 1));
                }
                PopupAction::None
            }
            KeyCode::Down => {
                let i = self.list_state.selected().unwrap_or(0);
                if i + 1 < self.items.len() {
                    self.list_state.select(Some(i + 1));
                }
                PopupAction::None
            }
            KeyCode::Char(' ') => {
                if let Some(i) = self.list_state.selected() {
                    self.items[i].enabled = !self.items[i].enabled;
                }
                PopupAction::Toggled
            }
            KeyCode::Enter => {
                self.confirmed = true;
                self.visible = false;
                PopupAction::Confirmed
            }
            KeyCode::Esc => {
                self.visible = false;
                PopupAction::Cancelled
            }
            _ => PopupAction::None,
        }
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.list_state.selected()
    }
}

pub enum PopupAction {
    None,
    Toggled,
    Confirmed,
    Cancelled,
}

pub struct PopupWidget<'a> {
    state: &'a mut PopupState,
}

impl<'a> PopupWidget<'a> {
    pub fn new(state: &'a mut PopupState) -> Self {
        Self { state }
    }

    pub fn render(self, area: Rect, buf: &mut Buffer) {
        let popup_width = (area.width as f32 * 0.6) as u16;
        let popup_height = (self.state.items.len() as u16 + 4).min(area.height - 4);
        let popup_area = Rect {
            x: (area.width - popup_width) / 2,
            y: (area.height - popup_height) / 2,
            width: popup_width,
            height: popup_height,
        };

        Clear.render(popup_area, buf);

        let items: Vec<ListItem> = self.state.items.iter().map(|item| {
            let marker = if item.enabled { "[x]" } else { "[ ]" };
            let text = format!("{} {} — {}", marker, item.label, item.detail);
            ListItem::new(text)
        }).collect();

        let list = List::new(items)
            .block(Block::default()
                .title(format!(" {} ", self.state.title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)))
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("▸ ");

        ratatui::widgets::StatefulWidget::render(list, popup_area, buf, &mut self.state.list_state);
    }
}
