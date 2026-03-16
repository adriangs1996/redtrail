use std::collections::HashMap;
use crate::db_v2::DbV2;
use crate::error::Error;

pub struct SqlCompleter {
    tables: Vec<String>,
    columns: HashMap<String, Vec<String>>,
}

impl SqlCompleter {
    pub fn from_db(db: &DbV2) -> Result<Self, Error> {
        let conn = db.conn();
        let mut tables = Vec::new();
        let mut columns = HashMap::new();

        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
        ).map_err(|e| Error::Db(e.to_string()))?;

        let table_names: Vec<String> = stmt.query_map([], |row| row.get(0))
            .map_err(|e| Error::Db(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        for table in &table_names {
            let mut col_stmt = conn.prepare(&format!("PRAGMA table_info({})", table))
                .map_err(|e| Error::Db(e.to_string()))?;
            let cols: Vec<String> = col_stmt.query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| Error::Db(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();
            columns.insert(table.clone(), cols);
            tables.push(table.clone());
        }

        Ok(Self { tables, columns })
    }

    pub fn complete(&self, input: &str) -> Vec<String> {
        let lower = input.to_lowercase();
        let trailing_space = input.ends_with(' ');
        let words: Vec<&str> = lower.split_whitespace().collect();

        if words.is_empty() { return vec![]; }

        if trailing_space {
            let last_word = words.last().unwrap_or(&"");
            match *last_word {
                "from" | "join" | "into" | "table" | "update" => self.tables.clone(),
                "where" | "and" | "or" | "on" | "set" => {
                    let table = self.find_table_in_query(&words);
                    match table {
                        Some(t) => self.columns.get(&t).cloned().unwrap_or_default(),
                        None => self.columns.values().flat_map(|c| c.iter()).cloned()
                            .collect::<std::collections::HashSet<_>>().into_iter().collect(),
                    }
                }
                _ => vec![],
            }
        } else {
            let partial = words.last().unwrap_or(&"");
            let prev_keyword = words.iter().rev().skip(1)
                .find(|w| matches!(w.to_lowercase().as_str(),
                    "from" | "join" | "into" | "table" | "update" |
                    "where" | "and" | "or" | "on" | "set"))
                .map(|w| w.to_lowercase());

            match prev_keyword.as_deref() {
                Some("from") | Some("join") | Some("into") | Some("table") | Some("update") => {
                    self.tables.iter().filter(|t| t.starts_with(partial)).cloned().collect()
                }
                Some("where") | Some("and") | Some("or") | Some("on") | Some("set") => {
                    let table = self.find_table_in_query(&words);
                    match table {
                        Some(t) => self.columns.get(&t).map(|cols| cols.iter().filter(|c| c.starts_with(partial)).cloned().collect()).unwrap_or_default(),
                        None => self.columns.values().flat_map(|cols| cols.iter()).filter(|c| c.starts_with(partial)).cloned().collect::<std::collections::HashSet<_>>().into_iter().collect(),
                    }
                }
                _ => vec![],
            }
        }
    }

    fn find_table_in_query(&self, words: &[&str]) -> Option<String> {
        for (i, word) in words.iter().enumerate() {
            if matches!(word.to_lowercase().as_str(), "from" | "update")
                && let Some(table) = words.get(i + 1)
                    && self.tables.contains(&table.to_string()) {
                        return Some(table.to_string());
                    }
        }
        None
    }
}
