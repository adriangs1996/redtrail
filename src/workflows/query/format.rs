use super::execute::QueryResult;

pub fn as_table(result: &QueryResult) -> String {
    if result.columns.is_empty() {
        return "(no columns)".to_string();
    }

    let mut widths: Vec<usize> = result.columns.iter().map(|c| c.len()).collect();
    for row in &result.rows {
        for (i, val) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(val.len());
            }
        }
    }

    let mut out = String::new();

    for (i, col) in result.columns.iter().enumerate() {
        if i > 0 { out.push_str(" | "); }
        out.push_str(&format!("{:width$}", col, width = widths[i]));
    }
    out.push('\n');

    for (i, w) in widths.iter().enumerate() {
        if i > 0 { out.push_str("-+-"); }
        out.push_str(&"-".repeat(*w));
    }
    out.push('\n');

    for row in &result.rows {
        for (i, val) in row.iter().enumerate() {
            if i > 0 { out.push_str(" | "); }
            out.push_str(&format!("{:width$}", val, width = widths[i]));
        }
        out.push('\n');
    }

    out.push_str(&format!("({} rows)", result.rows.len()));
    out
}
