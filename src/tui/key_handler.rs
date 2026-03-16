use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use super::channel::DriverCommand;
use super::widgets::{InputBar, LineKind, OutputPanel};

pub enum KeyAction {
    Quit,
    None,
}

pub async fn handle_key(
    key: KeyEvent,
    input: &mut InputBar,
    output: &mut OutputPanel,
    command_sender: &mpsc::Sender<DriverCommand>,
) -> KeyAction {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        let _ = command_sender.send(DriverCommand::Quit).await;
        return KeyAction::Quit;
    }

    match key.code {
        KeyCode::Enter => {
            let raw = input.take_input();
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() {
                return KeyAction::None;
            }
            if trimmed == "q" || trimmed == "quit" || trimmed == "exit" {
                let _ = command_sender.send(DriverCommand::Quit).await;
                return KeyAction::Quit;
            }

            output.push_line(format!("> {trimmed}"), LineKind::UserInput);
            output.auto_scroll = true;
            output.processing = true;
            let _ = command_sender.send(DriverCommand::Input(trimmed)).await;
        }
        KeyCode::Char(c) => input.insert_char(c),
        KeyCode::Backspace => input.delete_char(),
        KeyCode::Left => input.move_left(),
        KeyCode::Right => input.move_right(),
        KeyCode::Up => output.scroll_up(),
        KeyCode::Down => output.scroll_down(),
        _ => {}
    }

    KeyAction::None
}
