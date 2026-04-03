use crate::core::fmt::ascii;
use crate::error::Error;
use crate::extract::db::{get_entity, get_entity_observations_by_key, get_relationships_for};
use rusqlite::Connection;

pub struct EntityArgs<'a> {
    pub id: &'a str,
    pub relationships: bool,
    pub history: bool,
    pub json: bool,
}

pub fn run(conn: &Connection, args: &EntityArgs) -> Result<(), Error> {
    let entity = get_entity(conn, args.id).map_err(|_| {
        Error::Db(format!("entity not found: {}", args.id))
    })?;

    let rels = if args.relationships || args.json {
        Some(get_relationships_for(conn, &entity.id)?)
    } else {
        None
    };

    let observations = if args.history || args.json {
        Some(
            get_entity_observations_by_key(conn, &entity.entity_type, &entity.canonical_key)?,
        )
    } else {
        None
    };

    if args.json {
        print_json(&entity, rels.as_deref(), observations.as_deref())
    } else {
        print_detail(conn, &entity, rels.as_deref(), observations.as_deref())
    }
}

fn print_json(
    entity: &crate::extract::db::EntityRow,
    rels: Option<&[crate::extract::db::RelationshipRow]>,
    observations: Option<&[crate::extract::db::ObservationRow]>,
) -> Result<(), Error> {
    let props: Option<serde_json::Value> = entity
        .properties
        .as_deref()
        .and_then(|p| serde_json::from_str(p).ok());

    let rel_values: Vec<serde_json::Value> = rels
        .unwrap_or(&[])
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "source_entity_id": r.source_entity_id,
                "target_entity_id": r.target_entity_id,
                "relation_type": r.relation_type,
                "properties": r.properties.as_deref()
                    .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok()),
                "observed_at": r.observed_at,
            })
        })
        .collect();

    let obs_values: Vec<serde_json::Value> = observations
        .unwrap_or(&[])
        .iter()
        .map(|o| {
            serde_json::json!({
                "id": o.id,
                "command_id": o.command_id,
                "observed_at": o.observed_at,
                "context": o.context,
            })
        })
        .collect();

    let output = serde_json::json!({
        "id": entity.id,
        "type": entity.entity_type,
        "name": entity.name,
        "canonical_key": entity.canonical_key,
        "properties": props,
        "first_seen": entity.first_seen,
        "last_seen": entity.last_seen,
        "relationships": rel_values,
        "observations": obs_values,
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|e| Error::Db(e.to_string()))?
    );
    Ok(())
}

fn print_detail(
    _conn: &Connection,
    entity: &crate::extract::db::EntityRow,
    rels: Option<&[crate::extract::db::RelationshipRow]>,
    observations: Option<&[crate::extract::db::ObservationRow]>,
) -> Result<(), Error> {
    println!(
        "{BOLD}{CYAN}{name}{RESET}  {DIM}({type}){RESET}",
        BOLD = ascii::BOLD,
        CYAN = ascii::CYAN,
        RESET = ascii::RESET,
        DIM = ascii::DIM,
        name = entity.name,
        type = entity.entity_type,
    );
    println!();

    println!(
        "  {BOLD}ID:{RESET}            {id}",
        BOLD = ascii::BOLD,
        RESET = ascii::RESET,
        id = entity.id,
    );
    println!(
        "  {BOLD}Canonical Key:{RESET} {key}",
        BOLD = ascii::BOLD,
        RESET = ascii::RESET,
        key = entity.canonical_key,
    );
    println!(
        "  {BOLD}First Seen:{RESET}    {when}",
        BOLD = ascii::BOLD,
        RESET = ascii::RESET,
        when = ascii::format_relative_time(entity.first_seen),
    );
    println!(
        "  {BOLD}Last Seen:{RESET}     {when}",
        BOLD = ascii::BOLD,
        RESET = ascii::RESET,
        when = ascii::format_relative_time(entity.last_seen),
    );

    if let Some(props_str) = &entity.properties
        && let Ok(props) = serde_json::from_str::<serde_json::Value>(props_str)
        && let Some(obj) = props.as_object()
        && !obj.is_empty()
    {
        println!();
        println!(
            "  {BOLD}Properties:{RESET}",
            BOLD = ascii::BOLD,
            RESET = ascii::RESET,
        );
        for (k, v) in obj {
            println!(
                "    {DIM}{k}:{RESET} {v}",
                DIM = ascii::DIM,
                RESET = ascii::RESET,
            );
        }
    }

    if let Some(rels) = rels
        && !rels.is_empty()
    {
        println!();
        println!(
            "  {BOLD}Relationships:{RESET} {DIM}({count}){RESET}",
            BOLD = ascii::BOLD,
            RESET = ascii::RESET,
            DIM = ascii::DIM,
            count = rels.len(),
        );
        for r in rels {
            let direction = if r.source_entity_id == entity.id {
                format!("→ {}", r.target_entity_id)
            } else {
                format!("← {}", r.source_entity_id)
            };
            println!(
                "    {DIM}[{rtype}]{RESET} {dir}",
                DIM = ascii::DIM,
                RESET = ascii::RESET,
                rtype = r.relation_type,
                dir = direction,
            );
        }
    }

    if let Some(obs) = observations
        && !obs.is_empty()
    {
        println!();
        println!(
            "  {BOLD}Observation History:{RESET} {DIM}({count} observations){RESET}",
            BOLD = ascii::BOLD,
            RESET = ascii::RESET,
            DIM = ascii::DIM,
            count = obs.len(),
        );
        for o in obs.iter().rev().take(10) {
            let ctx = o.context.as_deref().unwrap_or("");
            println!(
                "    {YELLOW}{when}{RESET}  {DIM}{ctx}{RESET}",
                YELLOW = ascii::YELLOW,
                RESET = ascii::RESET,
                DIM = ascii::DIM,
                when = ascii::format_relative_time(o.observed_at),
                ctx = ascii::truncate_command(ctx, 60),
            );
        }
        if obs.len() > 10 {
            println!(
                "    {DIM}... ({} more){RESET}",
                obs.len() - 10,
                DIM = ascii::DIM,
                RESET = ascii::RESET,
            );
        }
    }

    Ok(())
}
