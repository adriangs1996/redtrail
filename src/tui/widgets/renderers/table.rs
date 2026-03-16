use crate::workflows::types::TableData;
use ratatui::prelude::*;

pub fn render(data: &TableData, width: u16) -> Vec<Line<'static>> {
    let w = width as usize;
    if data.headers.is_empty() {
        return vec![];
    }
    let col_count = data.headers.len();
    let border_chars = col_count + 1;

    let mut col_widths: Vec<usize> = data.headers.iter().map(|h| h.len()).collect();
    for row in &data.rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_widths.len() {
                col_widths[i] = col_widths[i].max(cell.len());
            }
        }
    }
    for cw in &mut col_widths {
        *cw += 2;
    }

    let total: usize = col_widths.iter().sum::<usize>() + border_chars;
    if total > w {
        let available = w.saturating_sub(border_chars);
        let min_col = 5usize;
        let total_natural: usize = col_widths.iter().sum();
        for cw in &mut col_widths {
            *cw = ((*cw as f64 / total_natural as f64) * available as f64).floor() as usize;
            if *cw < min_col {
                *cw = min_col;
            }
        }
    }

    let border_style = Style::default().fg(Color::DarkGray);
    let mut lines = Vec::new();

    lines.push(border_line('╭', '┬', '╮', '─', &col_widths, border_style));
    lines.push(data_line(
        &data.headers,
        &col_widths,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        border_style,
    ));
    lines.push(border_line('├', '┼', '┤', '─', &col_widths, border_style));
    for (i, row) in data.rows.iter().enumerate() {
        let bg = if i % 2 == 0 {
            None
        } else {
            Some(Color::Indexed(235))
        };
        let cell_style = if let Some(bg) = bg {
            Style::default().fg(Color::White).bg(bg)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(data_line(row, &col_widths, cell_style, border_style));
    }
    lines.push(border_line('╰', '┴', '╯', '─', &col_widths, border_style));

    lines
}

fn border_line(
    left: char,
    mid: char,
    right: char,
    fill: char,
    widths: &[usize],
    style: Style,
) -> Line<'static> {
    let mut s = String::new();
    s.push(left);
    for (i, w) in widths.iter().enumerate() {
        for _ in 0..*w {
            s.push(fill);
        }
        if i + 1 < widths.len() {
            s.push(mid);
        } else {
            s.push(right);
        }
    }
    Line::from(Span::styled(s, style))
}

fn data_line(
    cells: &[String],
    widths: &[usize],
    cell_style: Style,
    border_style: Style,
) -> Line<'static> {
    let mut spans = Vec::new();
    spans.push(Span::styled("│".to_string(), border_style));
    for (i, w) in widths.iter().enumerate() {
        let content = cells.get(i).map(|s| s.as_str()).unwrap_or("");
        let inner_w = w.saturating_sub(2);
        let display = if content.len() > inner_w && inner_w > 1 {
            format!(" {}… ", &content[..inner_w - 1])
        } else {
            format!(" {:width$} ", content, width = inner_w)
        };
        spans.push(Span::styled(display, cell_style));
        spans.push(Span::styled("│".to_string(), border_style));
    }
    Line::from(spans)
}
