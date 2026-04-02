// Entity resolution DB operations — upsert, relationship insertion, queries.

use crate::core::db::CommandRow;
use crate::error::Error;
use crate::extract::types::{NewEntity, NewRelationship, TypedEntityData};
use rusqlite::Connection;

// --- Row types ---

#[derive(Debug, Clone)]
pub struct EntityRow {
    pub id: String,
    pub entity_type: String,
    pub name: String,
    pub canonical_key: String,
    pub properties: Option<String>,
    pub first_seen: i64,
    pub last_seen: i64,
}

#[derive(Debug, Clone)]
pub struct RelationshipRow {
    pub id: String,
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relation_type: String,
    pub properties: Option<String>,
    pub observed_at: i64,
}

#[derive(Debug, Clone)]
pub struct ObservationRow {
    pub id: String,
    pub entity_id: String,
    pub command_id: String,
    pub observed_at: i64,
    pub context: Option<String>,
}

#[derive(Default)]
pub struct EntityFilter<'a> {
    pub entity_type: Option<&'a str>,
    pub limit: Option<usize>,
}

// --- Public API ---

/// Upsert an entity by (type, canonical_key). Creates if new, updates last_seen + properties if
/// exists. Also inserts an observation and typed table row if applicable.
/// Returns the entity ID (existing or newly created).
pub fn upsert_entity(
    conn: &Connection,
    entity: &NewEntity,
    command_id: &str,
    observed_at: i64,
) -> Result<String, Error> {
    let new_id = uuid::Uuid::new_v4().to_string();
    let props_json = entity
        .properties
        .as_ref()
        .map(|v| v.to_string());

    conn.execute(
        "INSERT INTO entities (id, type, name, canonical_key, properties, first_seen, last_seen, source_command_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)
         ON CONFLICT(type, canonical_key) DO UPDATE SET
             name = excluded.name,
             properties = excluded.properties,
             last_seen = excluded.last_seen",
        rusqlite::params![
            new_id,
            entity.entity_type,
            entity.name,
            entity.canonical_key,
            props_json,
            observed_at,
            command_id,
        ],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    // Retrieve the canonical entity_id (may be the pre-existing one on conflict)
    let entity_id: String = conn
        .query_row(
            "SELECT id FROM entities WHERE type = ?1 AND canonical_key = ?2",
            rusqlite::params![entity.entity_type, entity.canonical_key],
            |r| r.get(0),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    // Insert observation (ignore duplicate for same entity+command pair)
    let obs_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT OR IGNORE INTO entity_observations (id, entity_id, command_id, observed_at, context)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            obs_id,
            entity_id,
            command_id,
            observed_at,
            entity.observation_context,
        ],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    // Insert into typed table if typed_data is provided
    if let Some(ref typed) = entity.typed_data {
        insert_typed_data(conn, &entity_id, typed)?;
    }

    Ok(entity_id)
}

/// Insert a relationship between two entities identified by (type, canonical_key).
/// Resolves entity IDs internally. Returns the relationship ID.
pub fn insert_relationship(
    conn: &Connection,
    rel: &NewRelationship,
    command_id: &str,
    observed_at: i64,
) -> Result<String, Error> {
    let source_id: String = conn
        .query_row(
            "SELECT id FROM entities WHERE type = ?1 AND canonical_key = ?2",
            rusqlite::params![rel.source_type, rel.source_canonical_key],
            |r| r.get(0),
        )
        .map_err(|_| {
            Error::Db(format!(
                "source entity not found: type={}, key={}",
                rel.source_type, rel.source_canonical_key
            ))
        })?;

    let target_id: String = conn
        .query_row(
            "SELECT id FROM entities WHERE type = ?1 AND canonical_key = ?2",
            rusqlite::params![rel.target_type, rel.target_canonical_key],
            |r| r.get(0),
        )
        .map_err(|_| {
            Error::Db(format!(
                "target entity not found: type={}, key={}",
                rel.target_type, rel.target_canonical_key
            ))
        })?;

    let rel_id = uuid::Uuid::new_v4().to_string();
    let props_json = rel.properties.as_ref().map(|v| v.to_string());

    conn.execute(
        "INSERT OR IGNORE INTO relationships (id, source_entity_id, target_entity_id, type, properties, observed_at, source_command_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            rel_id,
            source_id,
            target_id,
            rel.relation_type,
            props_json,
            observed_at,
            command_id,
        ],
    )
    .map_err(|e| Error::Db(e.to_string()))?;

    Ok(rel_id)
}

/// Get a single entity by ID.
pub fn get_entity(conn: &Connection, id: &str) -> Result<EntityRow, Error> {
    conn.query_row(
        "SELECT id, type, name, canonical_key, properties, first_seen, last_seen FROM entities WHERE id = ?1",
        [id],
        |r| {
            Ok(EntityRow {
                id: r.get(0)?,
                entity_type: r.get(1)?,
                name: r.get(2)?,
                canonical_key: r.get(3)?,
                properties: r.get(4)?,
                first_seen: r.get(5)?,
                last_seen: r.get(6)?,
            })
        },
    )
    .map_err(|e| Error::Db(e.to_string()))
}

/// Query entities with optional filters.
pub fn get_entities(conn: &Connection, filter: &EntityFilter) -> Result<Vec<EntityRow>, Error> {
    let mut sql = String::from(
        "SELECT id, type, name, canonical_key, properties, first_seen, last_seen FROM entities WHERE 1=1",
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1usize;

    if let Some(et) = filter.entity_type {
        sql.push_str(&format!(" AND type = ?{idx}"));
        params.push(Box::new(et.to_string()));
        idx += 1;
    }
    let _ = idx; // consumed above; suppresses unused_assignments lint for future extensions

    sql.push_str(" ORDER BY last_seen DESC");

    let limit = filter.limit.unwrap_or(100);
    sql.push_str(&format!(" LIMIT {limit}"));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt
        .query_map(param_refs.as_slice(), |r| {
            Ok(EntityRow {
                id: r.get(0)?,
                entity_type: r.get(1)?,
                name: r.get(2)?,
                canonical_key: r.get(3)?,
                properties: r.get(4)?,
                first_seen: r.get(5)?,
                last_seen: r.get(6)?,
            })
        })
        .map_err(|e| Error::Db(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Db(e.to_string()))?);
    }
    Ok(result)
}

/// Get observations for an entity by its (type, canonical_key).
pub fn get_entity_observations_by_key(
    conn: &Connection,
    entity_type: &str,
    canonical_key: &str,
) -> Result<Vec<ObservationRow>, Error> {
    let entity_id: String = conn
        .query_row(
            "SELECT id FROM entities WHERE type = ?1 AND canonical_key = ?2",
            rusqlite::params![entity_type, canonical_key],
            |r| r.get(0),
        )
        .map_err(|_| {
            Error::Db(format!(
                "entity not found: type={entity_type}, key={canonical_key}"
            ))
        })?;

    let mut stmt = conn
        .prepare(
            "SELECT id, entity_id, command_id, observed_at, context
             FROM entity_observations
             WHERE entity_id = ?1
             ORDER BY observed_at ASC",
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    let rows = stmt
        .query_map([entity_id], |r| {
            Ok(ObservationRow {
                id: r.get(0)?,
                entity_id: r.get(1)?,
                command_id: r.get(2)?,
                observed_at: r.get(3)?,
                context: r.get(4)?,
            })
        })
        .map_err(|e| Error::Db(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Db(e.to_string()))?);
    }
    Ok(result)
}

/// Get relationships for an entity (both incoming and outgoing).
pub fn get_relationships_for(
    conn: &Connection,
    entity_id: &str,
) -> Result<Vec<RelationshipRow>, Error> {
    let mut stmt = conn
        .prepare(
            "SELECT id, source_entity_id, target_entity_id, type, properties, observed_at
             FROM relationships
             WHERE source_entity_id = ?1 OR target_entity_id = ?1
             ORDER BY observed_at DESC",
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    let rows = stmt
        .query_map([entity_id], |r| {
            Ok(RelationshipRow {
                id: r.get(0)?,
                source_entity_id: r.get(1)?,
                target_entity_id: r.get(2)?,
                relation_type: r.get(3)?,
                properties: r.get(4)?,
                observed_at: r.get(5)?,
            })
        })
        .map_err(|e| Error::Db(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Db(e.to_string()))?);
    }
    Ok(result)
}

/// Get commands that haven't been extracted yet.
pub fn get_unextracted_commands(
    conn: &Connection,
    since: Option<i64>,
    limit: usize,
) -> Result<Vec<CommandRow>, Error> {
    let mut sql = String::from(
        "SELECT id, session_id, command_raw, command_binary, cwd, exit_code, hostname, shell, source, timestamp_start, timestamp_end, stdout, stderr, stdout_truncated, stderr_truncated, redacted, stdout_compressed, stderr_compressed, tool_name, command_subcommand, git_repo, git_branch, agent_session_id
         FROM commands
         WHERE extracted = 0 AND status = 'finished'",
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(ts) = since {
        sql.push_str(&format!(" AND timestamp_start >= ?{idx}"));
        params.push(Box::new(ts));
        #[allow(unused_assignments)]
        {
            idx += 1;
        }
    }

    sql.push_str(" ORDER BY timestamp_start ASC");
    sql.push_str(&format!(" LIMIT {limit}"));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt
        .query_map(param_refs.as_slice(), map_command_row)
        .map_err(|e| Error::Db(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Db(e.to_string()))?);
    }
    Ok(result)
}

/// Mark a command as extracted with the given method.
pub fn mark_extracted(conn: &Connection, command_id: &str, method: &str) -> Result<(), Error> {
    conn.execute(
        "UPDATE commands SET extracted = 1, extraction_method = ?1 WHERE id = ?2",
        rusqlite::params![method, command_id],
    )
    .map_err(|e| Error::Db(e.to_string()))?;
    Ok(())
}

/// Fetch stdout/stderr for a command with transparent decompression.
/// Returns (stdout, stderr).
pub fn get_command_output(
    conn: &Connection,
    command_id: &str,
) -> Result<(Option<String>, Option<String>), Error> {
    let (stdout_text, stderr_text, stdout_blob, stderr_blob): (
        Option<String>,
        Option<String>,
        Option<Vec<u8>>,
        Option<Vec<u8>>,
    ) = conn
        .query_row(
            "SELECT stdout, stderr, stdout_compressed, stderr_compressed FROM commands WHERE id = ?1",
            [command_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|e| Error::Db(e.to_string()))?;

    let stdout = stdout_text.or_else(|| stdout_blob.as_deref().and_then(decompress_blob));
    let stderr = stderr_text.or_else(|| stderr_blob.as_deref().and_then(decompress_blob));
    Ok((stdout, stderr))
}

/// Fetch a single command by ID.
pub fn get_command_by_id(conn: &Connection, id: &str) -> Result<CommandRow, Error> {
    conn.query_row(
        "SELECT id, session_id, command_raw, command_binary, cwd, exit_code, hostname, shell, source, timestamp_start, timestamp_end, stdout, stderr, stdout_truncated, stderr_truncated, redacted, stdout_compressed, stderr_compressed, tool_name, command_subcommand, git_repo, git_branch, agent_session_id
         FROM commands WHERE id = ?1",
        [id],
        map_command_row,
    )
    .map_err(|e| Error::Db(e.to_string()))
}

// --- Internal helpers ---

fn decompress_blob(blob: &[u8]) -> Option<String> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    let mut decoder = ZlibDecoder::new(blob);
    let mut out = String::new();
    decoder.read_to_string(&mut out).ok()?;
    Some(out)
}

fn map_command_row(
    r: &rusqlite::Row<'_>,
) -> rusqlite::Result<CommandRow> {
    let stdout_text: Option<String> = r.get(11)?;
    let stderr_text: Option<String> = r.get(12)?;
    let stdout_compressed: Option<Vec<u8>> = r.get(16)?;
    let stderr_compressed: Option<Vec<u8>> = r.get(17)?;

    let stdout = stdout_text.or_else(|| stdout_compressed.as_deref().and_then(decompress_blob));
    let stderr = stderr_text.or_else(|| stderr_compressed.as_deref().and_then(decompress_blob));

    Ok(CommandRow {
        id: r.get(0)?,
        session_id: r.get(1)?,
        command_raw: r.get(2)?,
        command_binary: r.get(3)?,
        cwd: r.get(4)?,
        exit_code: r.get(5)?,
        hostname: r.get(6)?,
        shell: r.get(7)?,
        source: r.get(8)?,
        timestamp_start: r.get(9)?,
        timestamp_end: r.get(10)?,
        stdout,
        stderr,
        stdout_truncated: r.get(13)?,
        stderr_truncated: r.get(14)?,
        redacted: r.get(15)?,
        tool_name: r.get(18)?,
        command_subcommand: r.get(19)?,
        git_repo: r.get(20)?,
        git_branch: r.get(21)?,
        agent_session_id: r.get(22)?,
    })
}

fn insert_typed_data(
    conn: &Connection,
    entity_id: &str,
    typed: &TypedEntityData,
) -> Result<(), Error> {
    match typed {
        TypedEntityData::GitBranch {
            repo,
            name,
            is_remote,
            remote_name,
            upstream,
            ahead,
            behind,
            last_commit_hash,
        } => {
            conn.execute(
                "INSERT OR REPLACE INTO git_branches (entity_id, repo, name, is_remote, remote_name, upstream, ahead, behind, last_commit_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    entity_id, repo, name, is_remote, remote_name,
                    upstream, ahead, behind, last_commit_hash,
                ],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        TypedEntityData::GitCommit {
            repo,
            hash,
            short_hash,
            author_name,
            author_email,
            message,
            committed_at,
        } => {
            conn.execute(
                "INSERT OR REPLACE INTO git_commits (entity_id, repo, hash, short_hash, author_name, author_email, message, committed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    entity_id, repo, hash, short_hash,
                    author_name, author_email, message, committed_at,
                ],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        TypedEntityData::GitRemote { repo, name, url } => {
            conn.execute(
                "INSERT OR REPLACE INTO git_remotes (entity_id, repo, name, url)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![entity_id, repo, name, url],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        TypedEntityData::GitFile {
            repo,
            path,
            status,
            insertions,
            deletions,
        } => {
            conn.execute(
                "INSERT OR REPLACE INTO git_files (entity_id, repo, path, status, insertions, deletions)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![entity_id, repo, path, status, insertions, deletions],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        TypedEntityData::GitTag {
            repo,
            name,
            commit_hash,
        } => {
            conn.execute(
                "INSERT OR REPLACE INTO git_tags (entity_id, repo, name, commit_hash)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![entity_id, repo, name, commit_hash],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        TypedEntityData::GitStash {
            repo,
            index_num,
            message,
        } => {
            conn.execute(
                "INSERT OR REPLACE INTO git_stashes (entity_id, repo, index_num, message)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![entity_id, repo, index_num, message],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        TypedEntityData::DockerContainer {
            container_id,
            name,
            image,
            status,
            ports,
        } => {
            conn.execute(
                "INSERT OR REPLACE INTO docker_containers (entity_id, container_id, name, image, status, ports)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![entity_id, container_id, name, image, status, ports],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        TypedEntityData::DockerImage {
            repository,
            tag,
            image_id,
            size_bytes,
        } => {
            conn.execute(
                "INSERT OR REPLACE INTO docker_images (entity_id, repository, tag, image_id, size_bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![entity_id, repository, tag, image_id, size_bytes],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        TypedEntityData::DockerNetwork {
            name,
            network_id,
            driver,
        } => {
            conn.execute(
                "INSERT OR REPLACE INTO docker_networks (entity_id, name, network_id, driver)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![entity_id, name, network_id, driver],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        TypedEntityData::DockerVolume {
            name,
            driver,
            mountpoint,
        } => {
            conn.execute(
                "INSERT OR REPLACE INTO docker_volumes (entity_id, name, driver, mountpoint)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![entity_id, name, driver, mountpoint],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
        TypedEntityData::DockerService {
            name,
            image,
            compose_file,
            ports,
        } => {
            conn.execute(
                "INSERT OR REPLACE INTO docker_services (entity_id, name, image, compose_file, ports)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![entity_id, name, image, compose_file, ports],
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        }
    }
    Ok(())
}
