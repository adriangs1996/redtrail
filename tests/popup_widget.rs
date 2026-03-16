use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use redtrail::tui::widgets::popup::{PopupAction, PopupItem, PopupState};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn popup_navigates_with_arrows() {
    let items = vec![
        PopupItem { label: "A".into(), detail: "first".into(), enabled: true },
        PopupItem { label: "B".into(), detail: "second".into(), enabled: false },
        PopupItem { label: "C".into(), detail: "third".into(), enabled: true },
    ];
    let mut state = PopupState::new("Test".into(), items);

    assert_eq!(state.selected_index(), Some(0));

    state.handle_key(key(KeyCode::Down));
    assert_eq!(state.selected_index(), Some(1));

    state.handle_key(key(KeyCode::Down));
    assert_eq!(state.selected_index(), Some(2));

    state.handle_key(key(KeyCode::Down));
    assert_eq!(state.selected_index(), Some(2));

    state.handle_key(key(KeyCode::Up));
    assert_eq!(state.selected_index(), Some(1));
}

#[test]
fn popup_toggles_with_space() {
    let items = vec![
        PopupItem { label: "Tool".into(), detail: "desc".into(), enabled: true },
    ];
    let mut state = PopupState::new("Test".into(), items);

    assert!(state.items[0].enabled);

    let action = state.handle_key(key(KeyCode::Char(' ')));
    assert!(matches!(action, PopupAction::Toggled));
    assert!(!state.items[0].enabled);

    state.handle_key(key(KeyCode::Char(' ')));
    assert!(state.items[0].enabled);
}

#[test]
fn popup_confirms_with_enter() {
    let items = vec![
        PopupItem { label: "A".into(), detail: "".into(), enabled: true },
    ];
    let mut state = PopupState::new("Test".into(), items);

    let action = state.handle_key(key(KeyCode::Enter));
    assert!(matches!(action, PopupAction::Confirmed));
    assert!(!state.visible);
    assert!(state.confirmed);
}

#[test]
fn popup_cancels_with_esc() {
    let items = vec![
        PopupItem { label: "A".into(), detail: "".into(), enabled: true },
    ];
    let mut state = PopupState::new("Test".into(), items);

    let action = state.handle_key(key(KeyCode::Esc));
    assert!(matches!(action, PopupAction::Cancelled));
    assert!(!state.visible);
    assert!(!state.confirmed);
}

#[test]
fn popup_does_not_go_above_zero() {
    let items = vec![
        PopupItem { label: "A".into(), detail: "".into(), enabled: true },
    ];
    let mut state = PopupState::new("Test".into(), items);

    state.handle_key(key(KeyCode::Up));
    assert_eq!(state.selected_index(), Some(0));
}

#[test]
fn popup_rendering_does_not_panic() {
    use ratatui::{Terminal, backend::TestBackend};
    use redtrail::tui::widgets::popup::PopupWidget;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let items = vec![
        PopupItem { label: "nmap".into(), detail: "port scanner".into(), enabled: true },
        PopupItem { label: "gobuster".into(), detail: "dir bruteforcer".into(), enabled: false },
    ];
    let mut state = PopupState::new("Tools".into(), items);

    terminal.draw(|f| {
        let area = f.area();
        PopupWidget::new(&mut state).render(area, f.buffer_mut());
    }).unwrap();
}
