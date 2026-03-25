use super::FormatterEntry;

fn format_table(_columns: &[String], _rows: &[Vec<serde_json::Value>]) -> String {
    todo!()
}

inventory::submit! {
    FormatterEntry::new("table", format_table)
}
