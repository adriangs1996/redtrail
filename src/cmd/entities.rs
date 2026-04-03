use crate::core::fmt::ascii;
use crate::error::Error;
use crate::extract::db::{get_entities, EntityFilter};
use rusqlite::Connection;

pub struct EntitiesArgs<'a> {
    pub entity_type: Option<&'a str>,
    pub json: bool,
}

pub fn run(conn: &Connection, args: &EntitiesArgs) -> Result<(), Error> {
    let filter = EntityFilter {
        entity_type: args.entity_type,
        limit: Some(500),
    };
    let entities = get_entities(conn, &filter)?;

    if args.json {
        print_json(&entities)
    } else {
        print_table(&entities)
    }
}

fn print_json(entities: &[crate::extract::db::EntityRow]) -> Result<(), Error> {
    let entries: Vec<serde_json::Value> = entities
        .iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "type": e.entity_type,
                "name": e.name,
                "canonical_key": e.canonical_key,
                "properties": e.properties.as_deref()
                    .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok()),
                "first_seen": e.first_seen,
                "last_seen": e.last_seen,
            })
        })
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&entries).map_err(|e| Error::Db(e.to_string()))?
    );
    Ok(())
}

fn print_table(entities: &[crate::extract::db::EntityRow]) -> Result<(), Error> {
    if entities.is_empty() {
        println!("{}No entities found.{}", ascii::DIM, ascii::RESET);
        return Ok(());
    }

    // Group by entity_type for display
    let mut by_type: std::collections::BTreeMap<&str, Vec<&crate::extract::db::EntityRow>> =
        std::collections::BTreeMap::new();
    for e in entities {
        by_type.entry(e.entity_type.as_str()).or_default().push(e);
    }

    // Column widths
    let w_name = entities
        .iter()
        .map(|e| e.name.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let w_key = entities
        .iter()
        .map(|e| e.canonical_key.len())
        .max()
        .unwrap_or(13)
        .max(13)
        .min(50);
    let w_when: usize = 9; // "LAST SEEN" header

    let border = format!(
        "{DIM}+-{}-+-{}-+-{}-+{RESET}",
        "-".repeat(w_name),
        "-".repeat(w_key),
        "-".repeat(w_when),
        DIM = ascii::DIM,
        RESET = ascii::RESET,
    );

    for (entity_type, rows) in &by_type {
        println!(
            "\n{BOLD}{CYAN}{type}{RESET} {DIM}({count}){RESET}",
            BOLD = ascii::BOLD,
            CYAN = ascii::CYAN,
            RESET = ascii::RESET,
            DIM = ascii::DIM,
            type = entity_type,
            count = rows.len(),
        );

        println!("{border}");
        println!(
            "{DIM}|{RESET} {BOLD}{:<w_name$}{RESET} {DIM}|{RESET} {BOLD}{:<w_key$}{RESET} {DIM}|{RESET} {BOLD}{:>w_when$}{RESET} {DIM}|{RESET}",
            "NAME",
            "CANONICAL KEY",
            "LAST SEEN",
            BOLD = ascii::BOLD,
            DIM = ascii::DIM,
            RESET = ascii::RESET,
        );
        println!("{border}");

        for e in rows {
            let key_display = if e.canonical_key.len() > w_key {
                format!("{}...", &e.canonical_key[..w_key.saturating_sub(3)])
            } else {
                e.canonical_key.clone()
            };
            let when = ascii::format_relative_time(e.last_seen);
            println!(
                "{DIM}|{RESET} {:<w_name$} {DIM}|{RESET} {:<w_key$} {DIM}|{RESET} {YELLOW}{:>w_when$}{RESET} {DIM}|{RESET}",
                e.name,
                key_display,
                when,
                DIM = ascii::DIM,
                RESET = ascii::RESET,
                YELLOW = ascii::YELLOW,
            );
        }
        println!("{border}");
    }

    let total = entities.len();
    let type_count = by_type.len();
    println!(
        "\n{DIM}{total} entities across {type_count} types{RESET}",
        DIM = ascii::DIM,
        RESET = ascii::RESET,
    );

    Ok(())
}
