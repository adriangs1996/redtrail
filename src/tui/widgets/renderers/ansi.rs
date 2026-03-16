use ratatui::prelude::*;
use crate::workflows::{ShellOutputLine, ShellOutputStream};

pub fn render(lines: &[ShellOutputLine]) -> Vec<Line<'_>> {
    lines.iter().map(|l| {
        let base_fg = match l.stream {
            ShellOutputStream::Stderr => Some(Color::Red),
            ShellOutputStream::Stdout => None,
        };
        parse_ansi_line(&l.text, base_fg)
    }).collect()
}

fn parse_ansi_line(text: &str, base_fg: Option<Color>) -> Line<'_> {
    let mut spans = Vec::new();
    let mut current_style = Style::default();
    if let Some(fg) = base_fg {
        current_style = current_style.fg(fg);
    }
    let mut buf = String::new();
    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), current_style));
                buf.clear();
            }
            i += 2;
            let mut params = String::new();
            while i < bytes.len() {
                let b = bytes[i];
                if b.is_ascii_alphabetic() || b == b'~' {
                    i += 1;
                    break;
                }
                params.push(b as char);
                i += 1;
            }
            current_style = apply_sgr(&params, current_style, base_fg);
        } else if bytes[i] == b'\r' {
            i += 1;
        } else {
            buf.push(bytes[i] as char);
            i += 1;
        }
    }
    if !buf.is_empty() {
        spans.push(Span::styled(buf, current_style));
    }
    if spans.is_empty() {
        spans.push(Span::raw(""));
    }
    Line::from(spans)
}

fn apply_sgr(params: &str, current: Style, base_fg: Option<Color>) -> Style {
    let codes: Vec<u8> = params
        .split(';')
        .filter_map(|s| s.trim().parse::<u8>().ok())
        .collect();

    let mut style = current;
    let mut i = 0;
    while i < codes.len() {
        match codes[i] {
            0 => {
                style = Style::default();
                if let Some(fg) = base_fg {
                    style = style.fg(fg);
                }
            }
            1 => style = style.add_modifier(Modifier::BOLD),
            2 => style = style.add_modifier(Modifier::DIM),
            3 => style = style.add_modifier(Modifier::ITALIC),
            4 => style = style.add_modifier(Modifier::UNDERLINED),
            22 => style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            23 => style = style.remove_modifier(Modifier::ITALIC),
            24 => style = style.remove_modifier(Modifier::UNDERLINED),
            30..=37 => style = style.fg(sgr_color(codes[i] - 30)),
            38 => {
                if i + 2 < codes.len() && codes[i + 1] == 5 {
                    style = style.fg(Color::Indexed(codes[i + 2]));
                    i += 2;
                }
            }
            39 => { style = Style { fg: base_fg, ..style }; }
            40..=47 => style = style.bg(sgr_color(codes[i] - 40)),
            48 => {
                if i + 2 < codes.len() && codes[i + 1] == 5 {
                    style = style.bg(Color::Indexed(codes[i + 2]));
                    i += 2;
                }
            }
            49 => { style = Style { bg: None, ..style }; }
            90..=97 => style = style.fg(sgr_bright_color(codes[i] - 90)),
            100..=107 => style = style.bg(sgr_bright_color(codes[i] - 100)),
            _ => {}
        }
        i += 1;
    }
    style
}

fn sgr_color(n: u8) -> Color {
    match n {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::White,
        _ => Color::White,
    }
}

fn sgr_bright_color(n: u8) -> Color {
    match n {
        0 => Color::DarkGray,
        1 => Color::LightRed,
        2 => Color::LightGreen,
        3 => Color::LightYellow,
        4 => Color::LightBlue,
        5 => Color::LightMagenta,
        6 => Color::LightCyan,
        7 => Color::White,
        _ => Color::White,
    }
}
