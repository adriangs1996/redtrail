use ratatui::style::{Color, Modifier};
use redtrail::{ShellOutputLine, ShellOutputStream};

fn line(text: &str) -> ShellOutputLine {
    ShellOutputLine { text: text.to_string(), stream: ShellOutputStream::Stdout }
}

fn stderr_line(text: &str) -> ShellOutputLine {
    ShellOutputLine { text: text.to_string(), stream: ShellOutputStream::Stderr }
}

#[test]
fn md_plain_text() {
    let lines = vec![line("hello world")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].spans[0].content, "hello world");
}

#[test]
fn md_h1_bold_cyan() {
    let lines = vec![line("# Header")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let span = &result[0].spans[0];
    assert_eq!(span.style.fg, Some(Color::Cyan));
    assert!(span.style.add_modifier.contains(Modifier::BOLD));
    assert_eq!(span.content, "Header");
}

#[test]
fn md_h2() {
    let lines = vec![line("## Sub Header")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let span = &result[0].spans[0];
    assert_eq!(span.style.fg, Some(Color::Cyan));
    assert!(span.style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn md_bold() {
    let lines = vec![line("this is **bold** text")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let bold_span = result[0].spans.iter().find(|s| s.content == "bold").unwrap();
    assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn md_italic() {
    let lines = vec![line("this is *italic* text")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let italic_span = result[0].spans.iter().find(|s| s.content == "italic").unwrap();
    assert!(italic_span.style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn md_code_inline() {
    let lines = vec![line("use `foo()` here")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let code_span = result[0].spans.iter().find(|s| s.content == "foo()").unwrap();
    assert_eq!(code_span.style.fg, Some(Color::Green));
}

#[test]
fn md_code_block() {
    let lines = vec![
        line("```rust"),
        line("fn main() {}"),
        line("```"),
    ];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert!(result.len() >= 3);
    let code_line = &result[1];
    let has_bg = code_line.spans.iter().any(|s| s.style.bg.is_some());
    assert!(has_bg);
}

#[test]
fn md_code_block_no_lang() {
    let lines = vec![
        line("```"),
        line("plain code"),
        line("```"),
    ];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert!(result.len() >= 3);
}

#[test]
fn md_unordered_list() {
    let lines = vec![line("- item one"), line("- item two")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert_eq!(result.len(), 2);
    let first: String = result[0].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(first.contains("item one"));
}

#[test]
fn md_ordered_list() {
    let lines = vec![line("1. first"), line("2. second")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert_eq!(result.len(), 2);
}

#[test]
fn md_blockquote() {
    let lines = vec![line("> quoted text")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let text_span = result[0].spans.iter().find(|s| s.content.contains("quoted text")).unwrap();
    assert!(text_span.style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn md_link() {
    let lines = vec![line("[click here](https://example.com)")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let link_span = result[0].spans.iter().find(|s| s.content == "click here").unwrap();
    assert_eq!(link_span.style.fg, Some(Color::Cyan));
    assert!(link_span.style.add_modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn md_stderr_not_parsed_as_markdown() {
    let lines = vec![stderr_line("[tool] search {\"query\": \"test\"}")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert_eq!(result.len(), 1);
    let span = &result[0].spans[0];
    assert!(span.style.add_modifier.contains(Modifier::DIM));
}

#[test]
fn md_empty() {
    let lines: Vec<ShellOutputLine> = vec![];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert!(result.is_empty());
}

#[test]
fn md_mixed_inline() {
    let lines = vec![line("text **bold** and `code` end")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert!(result[0].spans.len() >= 4);
}

#[test]
fn md_h3() {
    let lines = vec![line("### Third Level")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let span = &result[0].spans[0];
    assert_eq!(span.style.fg, Some(Color::Cyan));
    assert!(span.style.add_modifier.contains(Modifier::BOLD));
    assert_eq!(span.content, "Third Level");
}

#[test]
fn md_bold_italic_combined() {
    let lines = vec![line("***bold and italic***")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let span = result[0].spans.iter().find(|s| s.content == "bold and italic").unwrap();
    assert!(span.style.add_modifier.contains(Modifier::BOLD));
    assert!(span.style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn md_inline_code_has_bg() {
    let lines = vec![line("use `foo()` here")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let code_span = result[0].spans.iter().find(|s| s.content == "foo()").unwrap();
    assert_eq!(code_span.style.fg, Some(Color::Green));
    assert!(code_span.style.bg.is_some());
}

#[test]
fn md_multiple_inline_codes() {
    let lines = vec![line("`a` and `b` and `c`")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let code_spans: Vec<_> = result[0].spans.iter()
        .filter(|s| s.style.fg == Some(Color::Green))
        .collect();
    assert_eq!(code_spans.len(), 3);
    assert_eq!(code_spans[0].content, "a");
    assert_eq!(code_spans[1].content, "b");
    assert_eq!(code_spans[2].content, "c");
}

#[test]
fn md_bold_then_italic_same_line() {
    let lines = vec![line("**bold** then *italic*")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let bold_span = result[0].spans.iter().find(|s| s.content == "bold").unwrap();
    assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
    let italic_span = result[0].spans.iter().find(|s| s.content == "italic").unwrap();
    assert!(italic_span.style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn md_code_block_with_known_language_has_syntax_colors() {
    let lines = vec![
        line("```python"),
        line("def hello():"),
        line("    return 42"),
        line("```"),
    ];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert_eq!(result.len(), 4);
    let code_line = &result[1];
    let has_colored_spans = code_line.spans.iter().any(|s| {
        matches!(s.style.fg, Some(Color::Rgb(_, _, _)))
    });
    assert!(has_colored_spans, "syntect should produce RGB colors");
}

#[test]
fn md_code_block_with_unknown_language_renders_plain() {
    let lines = vec![
        line("```xyz_unknown_lang"),
        line("some code here"),
        line("```"),
    ];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert!(result.len() >= 3);
    let code_line = &result[1];
    let has_bg = code_line.spans.iter().any(|s| s.style.bg.is_some());
    assert!(has_bg);
}

#[test]
fn md_code_block_multiline() {
    let lines = vec![
        line("```rust"),
        line("fn main() {"),
        line("    println!(\"hello\");"),
        line("}"),
        line("```"),
    ];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert_eq!(result.len(), 5);
    for i in 1..4 {
        let has_bg = result[i].spans.iter().any(|s| s.style.bg.is_some());
        assert!(has_bg, "code line {} should have bg", i);
    }
}

#[test]
fn md_unclosed_code_block_streaming() {
    let lines = vec![
        line("```rust"),
        line("fn partial()"),
    ];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert!(result.len() >= 2);
}

#[test]
fn md_unordered_list_has_bullet() {
    let lines = vec![line("- item")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let bullet = result[0].spans.iter().find(|s| s.content.contains("•"));
    assert!(bullet.is_some());
    let bullet_span = bullet.unwrap();
    assert_eq!(bullet_span.style.fg, Some(Color::Cyan));
}

#[test]
fn md_unordered_list_with_inline_formatting() {
    let lines = vec![line("- item with **bold**")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let bold_span = result[0].spans.iter().find(|s| s.content == "bold").unwrap();
    assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn md_ordered_list_number_is_cyan() {
    let lines = vec![line("1. first item")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let num_span = result[0].spans.iter().find(|s| s.content.contains("1.")).unwrap();
    assert_eq!(num_span.style.fg, Some(Color::Cyan));
}

#[test]
fn md_ordered_list_multidigit() {
    let lines = vec![line("10. tenth item")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let num_span = result[0].spans.iter().find(|s| s.content.contains("10.")).unwrap();
    assert_eq!(num_span.style.fg, Some(Color::Cyan));
}

#[test]
fn md_blockquote_has_bar() {
    let lines = vec![line("> text")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let bar = result[0].spans.iter().find(|s| s.content.contains("▎"));
    assert!(bar.is_some());
    let bar_span = bar.unwrap();
    assert_eq!(bar_span.style.fg, Some(Color::DarkGray));
}

#[test]
fn md_link_url_not_shown() {
    let lines = vec![line("[click](https://example.com)")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let all_text: String = result[0].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(all_text.contains("click"));
    assert!(!all_text.contains("https://"));
}

#[test]
fn md_plain_text_with_brackets_not_link() {
    let lines = vec![line("[not a link]")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let all_text: String = result[0].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(all_text.contains("[not a link]"));
}

#[test]
fn md_stderr_tool_result_dim_green() {
    let lines = vec![stderr_line("[result] search → found 3 items")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let span = &result[0].spans[0];
    assert_eq!(span.style.fg, Some(Color::Green));
    assert!(span.style.add_modifier.contains(Modifier::DIM));
}

#[test]
fn md_stderr_tool_call_dim_yellow() {
    let lines = vec![stderr_line("[tool] read_file {\"path\": \"test.rs\"}")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let span = &result[0].spans[0];
    assert_eq!(span.style.fg, Some(Color::Yellow));
    assert!(span.style.add_modifier.contains(Modifier::DIM));
}

#[test]
fn md_stderr_generic_dim_yellow() {
    let lines = vec![stderr_line("some stderr output")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let span = &result[0].spans[0];
    assert_eq!(span.style.fg, Some(Color::Yellow));
    assert!(span.style.add_modifier.contains(Modifier::DIM));
}

#[test]
fn md_mixed_stdout_stderr() {
    let lines = vec![
        line("# Response"),
        stderr_line("[tool] search"),
        line("Here are results"),
        stderr_line("[result] data"),
    ];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert_eq!(result.len(), 4);
    assert_eq!(result[0].spans[0].style.fg, Some(Color::Cyan));
    assert!(result[1].spans[0].style.add_modifier.contains(Modifier::DIM));
    assert_eq!(result[3].spans[0].style.fg, Some(Color::Green));
}

#[test]
fn md_multiline_mixed_content() {
    let lines = vec![
        line("# Title"),
        line(""),
        line("Some text with **bold** word"),
        line(""),
        line("- list item one"),
        line("- list item two"),
        line(""),
        line("> a quote"),
    ];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert_eq!(result.len(), 8);
    assert_eq!(result[0].spans[0].style.fg, Some(Color::Cyan));
    let bold = result[2].spans.iter().find(|s| s.content == "bold").unwrap();
    assert!(bold.style.add_modifier.contains(Modifier::BOLD));
    assert!(result[4].spans.iter().any(|s| s.content.contains("•")));
}

#[test]
fn md_header_only_hashes_no_text() {
    let lines = vec![line("# ")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert_eq!(result.len(), 1);
}

#[test]
fn md_line_starting_with_hash_no_space() {
    let lines = vec![line("#no_space")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    let all_text: String = result[0].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(all_text.contains("#no_space"));
}

#[test]
fn md_empty_line() {
    let lines = vec![line("")];
    let result = redtrail::tui::widgets::renderers::markdown::render(&lines);
    assert_eq!(result.len(), 1);
}
