mod table;

pub struct FormatterEntry {
    pub name: &'static str,
    pub format: fn(&[String], &[Vec<serde_json::Value>]) -> String,
}

impl FormatterEntry {
    pub const fn new(name: &'static str, format: fn(&[String], &[Vec<serde_json::Value>]) -> String) -> Self {
        Self { name, format }
    }
}

inventory::collect!(FormatterEntry);

pub fn format(name: &str, columns: &[String], rows: &[Vec<serde_json::Value>]) -> String {
    for entry in inventory::iter::<FormatterEntry> {
        if entry.name == name {
            return (entry.format)(columns, rows);
        }
    }
    for entry in inventory::iter::<FormatterEntry> {
        if entry.name == "table" {
            return (entry.format)(columns, rows);
        }
    }
    String::from("(no formatter found)")
}
