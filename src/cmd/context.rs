/// `redtrail context` — flagship command.
///
/// Synthesizes entity data for the current (or specified) git repo into a
/// project context snapshot suitable for pasting into an AI session or for
/// review before a commit.
use crate::error::Error;
use crate::extract::db::{get_entities, EntityFilter};
use rusqlite::Connection;

pub struct ContextArgs<'a> {
    pub format: &'a str,
    pub repo: Option<&'a str>,
}

pub fn run(conn: &Connection, args: &ContextArgs) -> Result<(), Error> {
    let repo_path = resolve_repo(args.repo);

    // All entities optionally filtered to this repo via canonical_key prefix
    let all_entities = get_entities(conn, &EntityFilter { entity_type: None, limit: Some(2000) })?;

    // Filter entities to the current repo when we have one.
    // Canonical keys for git entities are formed as "<repo>:<name>" so we can
    // match by prefix.
    let repo_prefix = repo_path
        .as_deref()
        .map(|r| format!("{r}:"));

    let entities: Vec<&crate::extract::db::EntityRow> = if let Some(ref prefix) = repo_prefix {
        all_entities
            .iter()
            .filter(|e| e.canonical_key.starts_with(prefix.as_str()))
            .collect()
    } else {
        all_entities.iter().collect()
    };

    // Gather sections
    let branches: Vec<_> = entities
        .iter()
        .filter(|e| e.entity_type == "git_branch")
        .collect();
    let commits: Vec<_> = {
        let mut cs: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == "git_commit")
            .collect();
        // Sort by last_seen desc (proxy for committed_at when typed table is unavailable)
        cs.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        cs.into_iter().take(5).collect()
    };
    let files: Vec<_> = entities
        .iter()
        .filter(|e| e.entity_type == "git_file")
        .collect();
    let remotes: Vec<_> = entities
        .iter()
        .filter(|e| e.entity_type == "git_remote")
        .collect();

    // Recent errors: query commands with non-zero exit_code from last 7 days
    let seven_days_ago = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        now - 7 * 24 * 3600
    };

    let recent_errors = query_recent_errors(conn, repo_path.as_deref(), seven_days_ago)?;

    match args.format {
        "json" => print_json(
            repo_path.as_deref(),
            &branches,
            &commits,
            &files,
            &remotes,
            &recent_errors,
        ),
        _ => print_markdown(
            repo_path.as_deref(),
            &branches,
            &commits,
            &files,
            &remotes,
            &recent_errors,
        ),
    }
}

// --- Repo detection ---

fn detect_repo(cwd: Option<&str>) -> Option<String> {
    let dir = cwd.unwrap_or(".");
    std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn resolve_repo(repo_arg: Option<&str>) -> Option<String> {
    match repo_arg {
        None | Some(".") => detect_repo(repo_arg),
        Some(path) => Some(path.to_string()),
    }
}

// --- Error query ---

#[derive(Debug)]
struct ErrorSummary {
    command_raw: String,
    exit_code: i32,
    timestamp_start: i64,
}

fn query_recent_errors(
    conn: &Connection,
    repo: Option<&str>,
    since: i64,
) -> Result<Vec<ErrorSummary>, Error> {
    let mut sql = String::from(
        "SELECT command_raw, exit_code, timestamp_start
         FROM commands
         WHERE exit_code != 0
           AND status = 'finished'
           AND timestamp_start >= ?1",
    );
    if let Some(r) = repo {
        sql.push_str(&format!(
            " AND git_repo = '{}'",
            r.replace('\'', "''")
        ));
    }
    sql.push_str(" ORDER BY timestamp_start DESC LIMIT 20");

    let mut stmt = conn.prepare(&sql).map_err(|e| Error::Db(e.to_string()))?;
    let rows = stmt
        .query_map([since], |r| {
            Ok(ErrorSummary {
                command_raw: r.get(0)?,
                exit_code: r.get(1).unwrap_or(-1),
                timestamp_start: r.get(2)?,
            })
        })
        .map_err(|e| Error::Db(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| Error::Db(e.to_string()))?);
    }
    Ok(result)
}

// --- Output formatters ---

fn print_markdown(
    repo: Option<&str>,
    branches: &[&&crate::extract::db::EntityRow],
    commits: &[&&crate::extract::db::EntityRow],
    files: &[&&crate::extract::db::EntityRow],
    remotes: &[&&crate::extract::db::EntityRow],
    errors: &[ErrorSummary],
) -> Result<(), Error> {
    if let Some(r) = repo {
        println!("# Project Context: {r}");
    } else {
        println!("# Project Context");
    }
    println!();

    // Branch section
    println!("## Branch");
    if branches.is_empty() {
        println!("_No branch information captured._");
    } else {
        // Try to find current branch from properties
        let current = branches.iter().find(|b| {
            b.properties
                .as_deref()
                .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok())
                .and_then(|v| v.get("is_current").and_then(|c| c.as_bool()))
                .unwrap_or(false)
        });
        if let Some(b) = current {
            println!("- `{}` (current)", b.name);
        }
        for b in branches.iter() {
            let is_current = b.properties
                .as_deref()
                .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok())
                .and_then(|v| v.get("is_current").and_then(|c| c.as_bool()))
                .unwrap_or(false);
            if !is_current {
                println!("- `{}`", b.name);
            }
        }
    }
    println!();

    // Recent commits
    println!("## Recent Commits");
    if commits.is_empty() {
        println!("_No commit history captured._");
    } else {
        for c in commits {
            println!("- `{}` — {}", c.canonical_key.split(':').last().unwrap_or(&c.name), c.name);
        }
    }
    println!();

    // Uncommitted changes
    println!("## Uncommitted Changes");
    let changed_files: Vec<_> = files
        .iter()
        .filter(|f| {
            f.properties
                .as_deref()
                .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok())
                .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(|s| !s.is_empty()))
                .unwrap_or(false)
        })
        .collect();
    if changed_files.is_empty() {
        println!("_No uncommitted changes captured._");
    } else {
        for f in &changed_files {
            let status = f.properties
                .as_deref()
                .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok())
                .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(String::from))
                .unwrap_or_default();
            println!("- `{}` {}", f.name, status);
        }
    }
    println!();

    // Remotes
    println!("## Remotes");
    if remotes.is_empty() {
        println!("_No remote information captured._");
    } else {
        for r in remotes {
            let url = r.properties
                .as_deref()
                .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok())
                .and_then(|v| v.get("url").and_then(|u| u.as_str()).map(String::from))
                .unwrap_or_default();
            if url.is_empty() {
                println!("- `{}`", r.name);
            } else {
                println!("- `{}` — {}", r.name, url);
            }
        }
    }
    println!();

    // Recent errors
    println!("## Recent Errors (last 7d)");
    if errors.is_empty() {
        println!("_No failed commands in the last 7 days._");
    } else {
        for e in errors {
            println!("- exit `{}` — `{}`", e.exit_code, truncate_str(&e.command_raw, 80));
        }
    }

    Ok(())
}

fn print_json(
    repo: Option<&str>,
    branches: &[&&crate::extract::db::EntityRow],
    commits: &[&&crate::extract::db::EntityRow],
    files: &[&&crate::extract::db::EntityRow],
    remotes: &[&&crate::extract::db::EntityRow],
    errors: &[ErrorSummary],
) -> Result<(), Error> {
    let to_json_entity =
        |e: &&&crate::extract::db::EntityRow| -> serde_json::Value {
            serde_json::json!({
                "id": e.id,
                "type": e.entity_type,
                "name": e.name,
                "canonical_key": e.canonical_key,
                "properties": e.properties.as_deref()
                    .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok()),
                "last_seen": e.last_seen,
            })
        };

    let output = serde_json::json!({
        "repo": repo,
        "branches": branches.iter().map(to_json_entity).collect::<Vec<_>>(),
        "recent_commits": commits.iter().map(to_json_entity).collect::<Vec<_>>(),
        "uncommitted_files": files.iter()
            .filter(|f| {
                f.properties
                    .as_deref()
                    .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok())
                    .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(|s| !s.is_empty()))
                    .unwrap_or(false)
            })
            .map(to_json_entity)
            .collect::<Vec<_>>(),
        "remotes": remotes.iter().map(to_json_entity).collect::<Vec<_>>(),
        "recent_errors": errors.iter().map(|e| serde_json::json!({
            "command": e.command_raw,
            "exit_code": e.exit_code,
            "timestamp": e.timestamp_start,
        })).collect::<Vec<_>>(),
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|e| Error::Db(e.to_string()))?
    );
    Ok(())
}

fn truncate_str(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}
