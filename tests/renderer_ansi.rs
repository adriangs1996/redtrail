use ratatui::style::{Color, Modifier};
use redtrail::{ShellOutputLine, ShellOutputStream};

#[test]
fn ansi_no_escapes() {
    let lines = vec![
        ShellOutputLine { text: "plain text".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].spans.len(), 1);
    assert_eq!(result[0].spans[0].content, "plain text");
}

#[test]
fn ansi_red_foreground() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[31mred text\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    assert_eq!(result.len(), 1);
    let red_span = result[0].spans.iter().find(|s| s.content == "red text").unwrap();
    assert_eq!(red_span.style.fg, Some(Color::Red));
}

#[test]
fn ansi_bold_green() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[1;32mbold green\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let span = result[0].spans.iter().find(|s| s.content == "bold green").unwrap();
    assert_eq!(span.style.fg, Some(Color::Green));
    assert!(span.style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn ansi_256_color() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[38;5;208morange\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let span = result[0].spans.iter().find(|s| s.content == "orange").unwrap();
    assert_eq!(span.style.fg, Some(Color::Indexed(208)));
}

#[test]
fn ansi_reset_mid_line() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[31mred\x1b[0mnormal".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    assert!(result[0].spans.len() >= 2);
    let red = result[0].spans.iter().find(|s| s.content == "red").unwrap();
    assert_eq!(red.style.fg, Some(Color::Red));
    let normal = result[0].spans.iter().find(|s| s.content == "normal").unwrap();
    assert_eq!(normal.style.fg, None);
}

#[test]
fn ansi_background_color() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[44mblue bg\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let span = result[0].spans.iter().find(|s| s.content == "blue bg").unwrap();
    assert_eq!(span.style.bg, Some(Color::Blue));
}

#[test]
fn ansi_underline() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[4munderlined\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let span = result[0].spans.iter().find(|s| s.content == "underlined").unwrap();
    assert!(span.style.add_modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn ansi_malformed_treated_as_text() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[999xinvalid".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    assert!(!result.is_empty());
    let all_text: String = result[0].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(all_text.contains("invalid"));
}

#[test]
fn ansi_stderr_preserved() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[32mgreen\x1b[0m".into(), stream: ShellOutputStream::Stderr },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    assert!(!result.is_empty());
}

#[test]
fn ansi_no_empty_spans_on_multiple_resets() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[0m\x1b[0m\x1b[0mtext".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    for span in &result[0].spans {
        assert!(!span.content.is_empty(), "found empty span");
    }
}

#[test]
fn ansi_multiple_lines() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[31mred\x1b[0m".into(), stream: ShellOutputStream::Stdout },
        ShellOutputLine { text: "\x1b[32mgreen\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    assert_eq!(result.len(), 2);
}

#[test]
fn ansi_dim_attribute() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[2mdimmed\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let span = result[0].spans.iter().find(|s| s.content == "dimmed").unwrap();
    assert!(span.style.add_modifier.contains(Modifier::DIM));
}

#[test]
fn ansi_italic_attribute() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[3mitalic\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let span = result[0].spans.iter().find(|s| s.content == "italic").unwrap();
    assert!(span.style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn ansi_combined_bold_italic_underline() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[1;3;4mstacked\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let span = result[0].spans.iter().find(|s| s.content == "stacked").unwrap();
    assert!(span.style.add_modifier.contains(Modifier::BOLD));
    assert!(span.style.add_modifier.contains(Modifier::ITALIC));
    assert!(span.style.add_modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn ansi_remove_bold_via_22() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[1mbold\x1b[22mnormal".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let bold = result[0].spans.iter().find(|s| s.content == "bold").unwrap();
    assert!(bold.style.add_modifier.contains(Modifier::BOLD));
    let normal = result[0].spans.iter().find(|s| s.content == "normal").unwrap();
    assert!(!normal.style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn ansi_remove_italic_via_23() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[3mitalic\x1b[23mnormal".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let italic = result[0].spans.iter().find(|s| s.content == "italic").unwrap();
    assert!(italic.style.add_modifier.contains(Modifier::ITALIC));
    let normal = result[0].spans.iter().find(|s| s.content == "normal").unwrap();
    assert!(!normal.style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn ansi_remove_underline_via_24() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[4munderline\x1b[24mnormal".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let under = result[0].spans.iter().find(|s| s.content == "underline").unwrap();
    assert!(under.style.add_modifier.contains(Modifier::UNDERLINED));
    let normal = result[0].spans.iter().find(|s| s.content == "normal").unwrap();
    assert!(!normal.style.add_modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn ansi_bright_foreground_colors() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[90mdark\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let span = result[0].spans.iter().find(|s| s.content == "dark").unwrap();
    assert_eq!(span.style.fg, Some(Color::DarkGray));
}

#[test]
fn ansi_bright_background_color() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[104mtext\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let span = result[0].spans.iter().find(|s| s.content == "text").unwrap();
    assert_eq!(span.style.bg, Some(Color::LightBlue));
}

#[test]
fn ansi_256_background_color() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[48;5;120mtext\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let span = result[0].spans.iter().find(|s| s.content == "text").unwrap();
    assert_eq!(span.style.bg, Some(Color::Indexed(120)));
}

#[test]
fn ansi_default_fg_reset_39() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[31mred\x1b[39mdefault".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let red = result[0].spans.iter().find(|s| s.content == "red").unwrap();
    assert_eq!(red.style.fg, Some(Color::Red));
    let def = result[0].spans.iter().find(|s| s.content == "default").unwrap();
    assert_eq!(def.style.fg, None);
}

#[test]
fn ansi_default_bg_reset_49() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[42mgreen bg\x1b[49mdefault bg".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let green = result[0].spans.iter().find(|s| s.content == "green bg").unwrap();
    assert_eq!(green.style.bg, Some(Color::Green));
    let def = result[0].spans.iter().find(|s| s.content == "default bg").unwrap();
    assert_eq!(def.style.bg, None);
}

#[test]
fn ansi_fg_and_bg_combined() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[31;44mred on blue\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let span = result[0].spans.iter().find(|s| s.content == "red on blue").unwrap();
    assert_eq!(span.style.fg, Some(Color::Red));
    assert_eq!(span.style.bg, Some(Color::Blue));
}

#[test]
fn ansi_multiple_color_changes_on_one_line() {
    let lines = vec![
        ShellOutputLine {
            text: "\x1b[31mred\x1b[32mgreen\x1b[34mblue\x1b[0m".into(),
            stream: ShellOutputStream::Stdout,
        },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let red = result[0].spans.iter().find(|s| s.content == "red").unwrap();
    assert_eq!(red.style.fg, Some(Color::Red));
    let green = result[0].spans.iter().find(|s| s.content == "green").unwrap();
    assert_eq!(green.style.fg, Some(Color::Green));
    let blue = result[0].spans.iter().find(|s| s.content == "blue").unwrap();
    assert_eq!(blue.style.fg, Some(Color::Blue));
}

#[test]
fn ansi_carriage_return_stripped() {
    let lines = vec![
        ShellOutputLine { text: "hello\rworld".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let all_text: String = result[0].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(!all_text.contains('\r'));
    assert!(all_text.contains("hello"));
    assert!(all_text.contains("world"));
}

#[test]
fn ansi_empty_line() {
    let lines = vec![
        ShellOutputLine { text: "".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    assert_eq!(result.len(), 1);
}

#[test]
fn ansi_only_escape_sequences_no_text() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[31m\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    assert_eq!(result.len(), 1);
    for span in &result[0].spans {
        if !span.content.is_empty() {
            panic!("expected no visible text, got: {:?}", span.content);
        }
    }
}

#[test]
fn ansi_text_before_first_escape() {
    let lines = vec![
        ShellOutputLine { text: "prefix\x1b[31mred\x1b[0m".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let prefix = result[0].spans.iter().find(|s| s.content == "prefix").unwrap();
    assert_eq!(prefix.style.fg, None);
    let red = result[0].spans.iter().find(|s| s.content == "red").unwrap();
    assert_eq!(red.style.fg, Some(Color::Red));
}

#[test]
fn ansi_text_after_reset_without_new_style() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[31mred\x1b[0m after".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let after = result[0].spans.iter().find(|s| s.content.contains("after")).unwrap();
    assert_eq!(after.style.fg, None);
}

#[test]
fn ansi_stderr_base_fg_restored_on_reset() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[32mgreen\x1b[0mback".into(), stream: ShellOutputStream::Stderr },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let green = result[0].spans.iter().find(|s| s.content == "green").unwrap();
    assert_eq!(green.style.fg, Some(Color::Green));
    let back = result[0].spans.iter().find(|s| s.content == "back").unwrap();
    assert_eq!(back.style.fg, Some(Color::Red));
}

#[test]
fn ansi_all_basic_fg_colors() {
    let colors = [
        (30, Color::Black), (31, Color::Red), (32, Color::Green),
        (33, Color::Yellow), (34, Color::Blue), (35, Color::Magenta),
        (36, Color::Cyan), (37, Color::White),
    ];
    for (code, expected) in colors {
        let text = format!("\x1b[{}mtext\x1b[0m", code);
        let lines = vec![ShellOutputLine { text, stream: ShellOutputStream::Stdout }];
        let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
        let span = result[0].spans.iter().find(|s| s.content == "text").unwrap();
        assert_eq!(span.style.fg, Some(expected), "failed for SGR code {}", code);
    }
}

#[test]
fn ansi_all_basic_bg_colors() {
    let colors = [
        (40, Color::Black), (41, Color::Red), (42, Color::Green),
        (43, Color::Yellow), (44, Color::Blue), (45, Color::Magenta),
        (46, Color::Cyan), (47, Color::White),
    ];
    for (code, expected) in colors {
        let text = format!("\x1b[{}mtext\x1b[0m", code);
        let lines = vec![ShellOutputLine { text, stream: ShellOutputStream::Stdout }];
        let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
        let span = result[0].spans.iter().find(|s| s.content == "text").unwrap();
        assert_eq!(span.style.bg, Some(expected), "failed for SGR code {}", code);
    }
}

#[test]
fn ansi_style_persists_across_text_segments() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[1;31mbold red text no reset".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let span = result[0].spans.iter().find(|s| s.content == "bold red text no reset").unwrap();
    assert_eq!(span.style.fg, Some(Color::Red));
    assert!(span.style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn ansi_incomplete_256_color_sequence() {
    let lines = vec![
        ShellOutputLine { text: "\x1b[38;5mtext".into(), stream: ShellOutputStream::Stdout },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    assert!(!result.is_empty());
    let all_text: String = result[0].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(all_text.contains("text"));
}

#[test]
fn ansi_realistic_ls_output() {
    let lines = vec![
        ShellOutputLine {
            text: "\x1b[0m\x1b[01;34msrc\x1b[0m  \x1b[01;34mtests\x1b[0m  \x1b[00mCargo.toml\x1b[0m".into(),
            stream: ShellOutputStream::Stdout,
        },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    assert!(!result.is_empty());
    let all_text: String = result[0].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(all_text.contains("src"));
    assert!(all_text.contains("tests"));
    assert!(all_text.contains("Cargo.toml"));
}

#[test]
fn ansi_realistic_grep_output() {
    let lines = vec![
        ShellOutputLine {
            text: "\x1b[35mfile.rs\x1b[0m\x1b[36m:\x1b[0m\x1b[32m42\x1b[0m\x1b[36m:\x1b[0m    let x = \x1b[1;31mfoo\x1b[0m();".into(),
            stream: ShellOutputStream::Stdout,
        },
    ];
    let result = redtrail::tui::widgets::renderers::ansi::render(&lines);
    let all_text: String = result[0].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(all_text.contains("file.rs"));
    assert!(all_text.contains("42"));
    assert!(all_text.contains("foo"));
}
