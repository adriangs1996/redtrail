use ratatui::style::{Color, Modifier};
use redtrail::TableData;

#[test]
fn table_basic_structure() {
    let data = TableData {
        headers: vec!["id".into(), "name".into()],
        rows: vec![
            vec!["1".into(), "test".into()],
            vec!["2".into(), "prod".into()],
        ],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 40);
    assert_eq!(result.len(), 6);
}

#[test]
fn table_top_border_rounded() {
    let data = TableData {
        headers: vec!["x".into()],
        rows: vec![vec!["1".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 20);
    let top: String = result[0].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(top.starts_with('╭'));
    assert!(top.ends_with('╮'));
}

#[test]
fn table_bottom_border_rounded() {
    let data = TableData {
        headers: vec!["x".into()],
        rows: vec![vec!["1".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 20);
    let bottom: String = result.last().unwrap().spans.iter().map(|s| s.content.to_string()).collect();
    assert!(bottom.starts_with('╰'));
    assert!(bottom.ends_with('╯'));
}

#[test]
fn table_header_bold_cyan() {
    let data = TableData {
        headers: vec!["name".into()],
        rows: vec![vec!["val".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 30);
    let header_spans = &result[1].spans;
    let name_span = header_spans.iter().find(|s| s.content.contains("name")).unwrap();
    assert_eq!(name_span.style.fg, Some(Color::Cyan));
    assert!(name_span.style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn table_alternating_rows() {
    let data = TableData {
        headers: vec!["x".into()],
        rows: vec![
            vec!["a".into()],
            vec!["b".into()],
            vec!["c".into()],
        ],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 20);
    let row0_bg = result[3].spans.iter().find(|s| s.content.contains("a")).map(|s| s.style.bg);
    let row1_bg = result[4].spans.iter().find(|s| s.content.contains("b")).map(|s| s.style.bg);
    assert_ne!(row0_bg, row1_bg);
}

#[test]
fn table_empty_rows() {
    let data = TableData {
        headers: vec!["id".into(), "val".into()],
        rows: vec![],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 30);
    assert_eq!(result.len(), 4);
}

#[test]
fn table_single_column() {
    let data = TableData {
        headers: vec!["x".into()],
        rows: vec![vec!["hello".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 20);
    assert_eq!(result.len(), 5);
}

#[test]
fn table_truncation() {
    let data = TableData {
        headers: vec!["name".into()],
        rows: vec![vec!["a very long string that exceeds width".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 15);
    let row: String = result[3].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(row.contains('…'));
}

#[test]
fn table_empty_headers_returns_empty() {
    let data = TableData {
        headers: vec![],
        rows: vec![vec!["data".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 40);
    assert!(result.is_empty());
}

#[test]
fn table_separator_has_correct_chars() {
    let data = TableData {
        headers: vec!["a".into(), "b".into()],
        rows: vec![vec!["1".into(), "2".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 40);
    let sep: String = result[2].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(sep.starts_with('├'));
    assert!(sep.ends_with('┤'));
    assert!(sep.contains('┼'));
}

#[test]
fn table_top_border_has_junction() {
    let data = TableData {
        headers: vec!["a".into(), "b".into()],
        rows: vec![],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 40);
    let top: String = result[0].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(top.contains('┬'));
}

#[test]
fn table_bottom_border_has_junction() {
    let data = TableData {
        headers: vec!["a".into(), "b".into()],
        rows: vec![],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 40);
    let bottom: String = result.last().unwrap().spans.iter().map(|s| s.content.to_string()).collect();
    assert!(bottom.contains('┴'));
}

#[test]
fn table_data_rows_have_pipe_borders() {
    let data = TableData {
        headers: vec!["x".into()],
        rows: vec![vec!["val".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 30);
    let row_spans = &result[3].spans;
    let first_border = row_spans.first().unwrap();
    assert_eq!(first_border.content, "│");
    let last_border = row_spans.last().unwrap();
    assert_eq!(last_border.content, "│");
}

#[test]
fn table_header_row_has_pipe_borders() {
    let data = TableData {
        headers: vec!["name".into()],
        rows: vec![],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 30);
    let header_spans = &result[1].spans;
    assert_eq!(header_spans.first().unwrap().content, "│");
    assert_eq!(header_spans.last().unwrap().content, "│");
}

#[test]
fn table_borders_are_dark_gray() {
    let data = TableData {
        headers: vec!["x".into()],
        rows: vec![vec!["1".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 20);
    let top_span = &result[0].spans[0];
    assert_eq!(top_span.style.fg, Some(Color::DarkGray));
}

#[test]
fn table_data_row_text_is_white() {
    let data = TableData {
        headers: vec!["x".into()],
        rows: vec![vec!["hello".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 30);
    let cell_span = result[3].spans.iter().find(|s| s.content.contains("hello")).unwrap();
    assert_eq!(cell_span.style.fg, Some(Color::White));
}

#[test]
fn table_many_columns() {
    let data = TableData {
        headers: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
        rows: vec![vec!["1".into(), "2".into(), "3".into(), "4".into(), "5".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 80);
    assert_eq!(result.len(), 5);
}

#[test]
fn table_wide_enough_no_truncation() {
    let data = TableData {
        headers: vec!["id".into(), "name".into()],
        rows: vec![vec!["1".into(), "alice".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 80);
    let row: String = result[3].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(!row.contains('…'));
    assert!(row.contains("alice"));
}

#[test]
fn table_missing_cells_in_row() {
    let data = TableData {
        headers: vec!["a".into(), "b".into(), "c".into()],
        rows: vec![vec!["1".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 60);
    assert_eq!(result.len(), 5);
}

#[test]
fn table_very_narrow_width() {
    let data = TableData {
        headers: vec!["name".into(), "value".into()],
        rows: vec![vec!["test".into(), "12345".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 10);
    assert!(!result.is_empty());
}

#[test]
fn table_one_row_even_bg() {
    let data = TableData {
        headers: vec!["x".into()],
        rows: vec![vec!["a".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 20);
    let cell = result[3].spans.iter().find(|s| s.content.contains("a")).unwrap();
    assert_eq!(cell.style.bg, None);
}

#[test]
fn table_second_row_has_bg() {
    let data = TableData {
        headers: vec!["x".into()],
        rows: vec![vec!["a".into()], vec!["b".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 20);
    let cell = result[4].spans.iter().find(|s| s.content.contains("b")).unwrap();
    assert_eq!(cell.style.bg, Some(Color::Indexed(235)));
}

#[test]
fn table_third_row_no_bg() {
    let data = TableData {
        headers: vec!["x".into()],
        rows: vec![vec!["a".into()], vec!["b".into()], vec!["c".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 20);
    let cell = result[5].spans.iter().find(|s| s.content.contains("c")).unwrap();
    assert_eq!(cell.style.bg, None);
}

#[test]
fn table_header_content_preserved() {
    let data = TableData {
        headers: vec!["status".into(), "count".into()],
        rows: vec![],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 40);
    let header: String = result[1].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(header.contains("status"));
    assert!(header.contains("count"));
}

#[test]
fn table_cell_content_preserved() {
    let data = TableData {
        headers: vec!["x".into()],
        rows: vec![vec!["hello world".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 40);
    let row: String = result[3].spans.iter().map(|s| s.content.to_string()).collect();
    assert!(row.contains("hello world"));
}

#[test]
fn table_line_count_matches_data_model() {
    let data = TableData {
        headers: vec!["a".into()],
        rows: vec![vec!["1".into()], vec!["2".into()], vec!["3".into()], vec!["4".into()], vec!["5".into()]],
    };
    let result = redtrail::tui::widgets::renderers::table::render(&data, 30);
    assert_eq!(result.len(), 5 + 4);
}
