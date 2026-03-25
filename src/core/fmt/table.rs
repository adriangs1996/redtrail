use super::FormatterEntry;

fn format_table(columns: &[String], rows: &[Vec<serde_json::Value>]) -> String {
    if columns.is_empty() {
        return String::from("(0 rows)\n");
    }

    let str_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|v| match v {
                    serde_json::Value::Null => "NULL".to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    other => other.to_string(),
                })
                .collect()
        })
        .collect();

    let mut widths: Vec<usize> = columns.iter().map(|c| c.len()).collect();
    for row in &str_rows {
        for (i, val) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(val.len());
            }
        }
    }

    let mut out = String::new();

    // Header
    let header: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{:<width$}", c, width = widths[i]))
        .collect();
    out.push_str(&header.join(" | "));
    out.push('\n');

    // Separator
    let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    out.push_str(&sep.join("-+-"));
    out.push('\n');

    // Rows
    for row in &str_rows {
        let formatted: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let w = if i < widths.len() { widths[i] } else { v.len() };
                format!("{:<width$}", v, width = w)
            })
            .collect();
        out.push_str(&formatted.join(" | "));
        out.push('\n');
    }

    out.push_str(&format!("({} rows)\n", rows.len()));
    out
}

inventory::submit! {
    FormatterEntry::new("table", format_table)
}
