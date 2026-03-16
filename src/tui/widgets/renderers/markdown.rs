use std::sync::LazyLock;
use ratatui::prelude::*;
use syntect::highlighting::{ThemeSet, Style as SyntectStyle};
use syntect::parsing::SyntaxSet;
use syntect::easy::HighlightLines;
use crate::workflows::{ShellOutputLine, ShellOutputStream};

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

pub fn render(lines: &[ShellOutputLine]) -> Vec<Line<'static>> {
    let mut result = Vec::new();
    let mut in_code_block = false;
    let mut code_lang: Option<String> = None;
    let mut code_lines: Vec<String> = Vec::new();

    for line in lines {
        if line.stream == ShellOutputStream::Stderr {
            let style = if line.text.starts_with("[tool]") {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::DIM)
            } else if line.text.starts_with("[result]") {
                Style::default().fg(Color::Green).add_modifier(Modifier::DIM)
            } else {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::DIM)
            };
            result.push(Line::from(Span::styled(line.text.clone(), style)));
            continue;
        }

        let text = &line.text;

        if text.starts_with("```") {
            if in_code_block {
                result.extend(render_code_block(&code_lines, &code_lang));
                code_lines.clear();
                code_lang = None;
                in_code_block = false;
                let bg = Color::Indexed(236);
                result.push(Line::from(Span::styled(text.clone(), Style::default().bg(bg).fg(Color::DarkGray))));
            } else {
                in_code_block = true;
                let lang = text.trim_start_matches('`').trim();
                code_lang = if lang.is_empty() { None } else { Some(lang.to_string()) };
                let bg = Color::Indexed(236);
                result.push(Line::from(Span::styled(text.clone(), Style::default().bg(bg).fg(Color::DarkGray))));
            }
            continue;
        }

        if in_code_block {
            code_lines.push(text.clone());
            continue;
        }

        result.push(render_markdown_line(text));
    }

    if in_code_block && !code_lines.is_empty() {
        result.extend(render_code_block(&code_lines, &code_lang));
    }

    result
}

fn render_code_block(lines: &[String], lang: &Option<String>) -> Vec<Line<'static>> {
    let bg = Color::Indexed(236);
    let theme = &THEME_SET.themes["base16-ocean.dark"];

    if let Some(lang_str) = lang {
        if let Some(syntax) = SYNTAX_SET.find_syntax_by_token(lang_str) {
            let mut h = HighlightLines::new(syntax, theme);
            return lines.iter().map(|line_text| {
                let regions = h.highlight_line(line_text, &SYNTAX_SET).unwrap_or_default();
                let spans: Vec<Span<'static>> = regions.into_iter().map(|(style, text)| {
                    Span::styled(text.to_string(), syntect_to_ratatui(style, bg))
                }).collect();
                if spans.is_empty() {
                    Line::from(Span::styled(line_text.clone(), Style::default().bg(bg)))
                } else {
                    Line::from(spans)
                }
            }).collect();
        }
    }

    lines.iter().map(|line_text| {
        Line::from(Span::styled(line_text.clone(), Style::default().bg(bg).fg(Color::White)))
    }).collect()
}

fn syntect_to_ratatui(style: SyntectStyle, bg: Color) -> ratatui::style::Style {
    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
    ratatui::style::Style::default().fg(fg).bg(bg)
}

fn render_markdown_line(text: &str) -> Line<'static> {
    if text.starts_with("### ") {
        return Line::from(Span::styled(
            text[4..].to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
    }
    if text.starts_with("## ") {
        return Line::from(Span::styled(
            text[3..].to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
    }
    if text.starts_with("# ") {
        return Line::from(Span::styled(
            text[2..].to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
    }

    if text.starts_with("> ") {
        return Line::from(vec![
            Span::styled("▎ ".to_string(), Style::default().fg(Color::DarkGray)),
            Span::styled(text[2..].to_string(), Style::default().add_modifier(Modifier::ITALIC)),
        ]);
    }

    if text.starts_with("- ") {
        let mut spans = vec![
            Span::styled("  ".to_string(), Style::default()),
            Span::styled("• ".to_string(), Style::default().fg(Color::Cyan)),
        ];
        spans.extend(parse_inline(&text[2..]));
        return Line::from(spans);
    }

    if let Some(rest) = parse_ordered_list(text) {
        let (num, content) = rest;
        let mut spans = vec![
            Span::styled("  ".to_string(), Style::default()),
            Span::styled(format!("{}. ", num), Style::default().fg(Color::Cyan)),
        ];
        spans.extend(parse_inline(content));
        return Line::from(spans);
    }

    Line::from(parse_inline(text))
}

fn parse_ordered_list(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0 && i < bytes.len() && bytes[i] == b'.' && i + 1 < bytes.len() && bytes[i + 1] == b' ' {
        Some((&text[..i], &text[i + 2..]))
    } else {
        None
    }
}

fn parse_inline(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut buf = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if i + 2 < chars.len() && chars[i] == '*' && chars[i+1] == '*' && chars[i+2] == '*' {
            if !buf.is_empty() {
                spans.push(Span::raw(buf.clone()));
                buf.clear();
            }
            i += 3;
            let mut inner = String::new();
            while i + 2 < chars.len() && !(chars[i] == '*' && chars[i+1] == '*' && chars[i+2] == '*') {
                inner.push(chars[i]);
                i += 1;
            }
            if i + 2 < chars.len() { i += 3; }
            spans.push(Span::styled(inner, Style::default().add_modifier(Modifier::BOLD | Modifier::ITALIC)));
            continue;
        }

        if i + 1 < chars.len() && chars[i] == '*' && chars[i+1] == '*' {
            if !buf.is_empty() {
                spans.push(Span::raw(buf.clone()));
                buf.clear();
            }
            i += 2;
            let mut inner = String::new();
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i+1] == '*') {
                inner.push(chars[i]);
                i += 1;
            }
            if i + 1 < chars.len() { i += 2; }
            spans.push(Span::styled(inner, Style::default().add_modifier(Modifier::BOLD)));
            continue;
        }

        if chars[i] == '*' && (i + 1 < chars.len() && chars[i+1] != '*') {
            if !buf.is_empty() {
                spans.push(Span::raw(buf.clone()));
                buf.clear();
            }
            i += 1;
            let mut inner = String::new();
            while i < chars.len() && chars[i] != '*' {
                inner.push(chars[i]);
                i += 1;
            }
            if i < chars.len() { i += 1; }
            spans.push(Span::styled(inner, Style::default().add_modifier(Modifier::ITALIC)));
            continue;
        }

        if chars[i] == '`' {
            if !buf.is_empty() {
                spans.push(Span::raw(buf.clone()));
                buf.clear();
            }
            i += 1;
            let mut inner = String::new();
            while i < chars.len() && chars[i] != '`' {
                inner.push(chars[i]);
                i += 1;
            }
            if i < chars.len() { i += 1; }
            spans.push(Span::styled(inner, Style::default().fg(Color::Green).bg(Color::Indexed(236))));
            continue;
        }

        if chars[i] == '[' {
            let start = i;
            i += 1;
            let mut link_text = String::new();
            while i < chars.len() && chars[i] != ']' {
                link_text.push(chars[i]);
                i += 1;
            }
            if i + 1 < chars.len() && chars[i] == ']' && chars[i+1] == '(' {
                i += 2;
                while i < chars.len() && chars[i] != ')' { i += 1; }
                if i < chars.len() { i += 1; }
                if !buf.is_empty() {
                    spans.push(Span::raw(buf.clone()));
                    buf.clear();
                }
                spans.push(Span::styled(link_text, Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED)));
                continue;
            } else {
                i = start;
                buf.push(chars[i]);
                i += 1;
                continue;
            }
        }

        buf.push(chars[i]);
        i += 1;
    }

    if !buf.is_empty() {
        spans.push(Span::raw(buf));
    }
    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}
