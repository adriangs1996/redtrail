/// Commands sent from the TUI to the Driver.
#[derive(Debug, Clone)]
pub enum DriverCommand {
    /// User submitted input to process.
    Input(String),
    Quit,
}

/// Events sent from the Driver back to the TUI.
#[derive(Debug, Clone)]
pub enum DriverEvent {
    /// A chunk of streaming output (token-by-token).
    Token(String),
    /// Processing finished for the current request.
    Done,
    /// An error occurred.
    Error(String),
    /// Input mode changed.
    ModeChanged(InputMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Chat,
    Terminal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_command_variants() {
        let cmd = DriverCommand::Input("test".into());
        assert!(matches!(cmd, DriverCommand::Input(_)));
        assert!(matches!(DriverCommand::Quit, DriverCommand::Quit));
    }

    #[test]
    fn test_driver_event_variants() {
        let event = DriverEvent::Token("hello".into());
        assert!(matches!(event, DriverEvent::Token(_)));
        assert!(matches!(DriverEvent::Done, DriverEvent::Done));
        let event = DriverEvent::Error("fail".into());
        assert!(matches!(event, DriverEvent::Error(_)));
    }
}
