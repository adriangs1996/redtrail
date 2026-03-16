use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::agent::knowledge::{ScanSession, SessionStatus, SessionSummary};
use crate::error::Error;

/// A successful attack pattern learned across sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackPattern {
    #[serde(default)]
    pub id: i64,
    pub technique: String,
    pub vulnerability_class: String,
    pub service_type: String,
    #[serde(default)]
    pub technology_stack: String,
    #[serde(default)]
    pub total_attempts: u32,
    #[serde(default)]
    pub successes: u32,
    #[serde(default)]
    pub avg_tool_calls: f64,
    #[serde(default)]
    pub avg_duration_secs: f64,
    #[serde(default)]
    pub brute_force_needed: bool,
    #[serde(default)]
    pub attack_chain: String,
    #[serde(default)]
    pub first_seen_at: String,
    #[serde(default)]
    pub last_seen_at: String,
    #[serde(default)]
    pub last_session_id: String,
}

/// A single technique execution record for per-task metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechniqueExecution {
    #[serde(default)]
    pub id: i64,
    pub session_id: String,
    pub task_type: String,
    pub target_host: String,
    #[serde(default)]
    pub target_service: String,
    #[serde(default)]
    pub tool_calls: u32,
    #[serde(default)]
    pub wall_clock_secs: f64,
    #[serde(default)]
    pub succeeded: bool,
    #[serde(default)]
    pub brute_force_used: bool,
    #[serde(default)]
    pub technology_stack: String,
    #[serde(default)]
    pub executed_at: String,
}

/// A fingerprint summarizing a past session for similarity matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFingerprint {
    pub session_id: String,
    #[serde(default)]
    pub services_seen: String,
    #[serde(default)]
    pub technologies_seen: String,
    #[serde(default)]
    pub vuln_classes_found: String,
    #[serde(default)]
    pub flags_captured: u32,
    #[serde(default)]
    pub hosts_count: u32,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub goal_type: String,
}

/// Composite cross-session intelligence returned by `gather_cross_session_intel`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSessionIntel {
    pub relevant_patterns: Vec<AttackPattern>,
    pub similar_sessions: Vec<SessionFingerprint>,
    pub technique_stats: Vec<TechniqueStats>,
}

/// Aggregated statistics for a technique type across sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechniqueStats {
    pub task_type: String,
    pub avg_tool_calls: f64,
    pub success_rate: f64,
    pub avg_duration: f64,
}

/// Query parameters for finding relevant cross-session intelligence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelevanceQuery {
    #[serde(default)]
    pub services: Vec<String>,
    #[serde(default)]
    pub technologies: Vec<String>,
    #[serde(default)]
    pub goal_type: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Sequential schema migrations. Each entry is (version, SQL).
/// Migrations use `IF NOT EXISTS` / `IF EXISTS` to be idempotent.
const MIGRATIONS: &[(u32, &str)] = &[
    (
        0,
        "CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            target_url TEXT,
            target_hosts TEXT NOT NULL,
            total_turns_used INTEGER NOT NULL,
            max_turns_configured INTEGER NOT NULL,
            llm_provider TEXT NOT NULL,
            knowledge TEXT NOT NULL,
            findings TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'completed'
        );",
    ),
    (
        1,
        "CREATE TABLE IF NOT EXISTS attack_patterns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            technique TEXT NOT NULL,
            vulnerability_class TEXT NOT NULL,
            service_type TEXT NOT NULL,
            technology_stack TEXT NOT NULL DEFAULT '',
            total_attempts INTEGER NOT NULL DEFAULT 0,
            successes INTEGER NOT NULL DEFAULT 0,
            avg_tool_calls REAL NOT NULL DEFAULT 0.0,
            avg_duration_secs REAL NOT NULL DEFAULT 0.0,
            brute_force_needed INTEGER NOT NULL DEFAULT 0,
            attack_chain TEXT NOT NULL DEFAULT '[]',
            first_seen_at TEXT NOT NULL,
            last_seen_at TEXT NOT NULL,
            last_session_id TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS technique_executions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            task_type TEXT NOT NULL,
            target_host TEXT NOT NULL,
            target_service TEXT NOT NULL DEFAULT '',
            tool_calls INTEGER NOT NULL DEFAULT 0,
            wall_clock_secs REAL NOT NULL DEFAULT 0.0,
            succeeded INTEGER NOT NULL DEFAULT 0,
            brute_force_used INTEGER NOT NULL DEFAULT 0,
            technology_stack TEXT NOT NULL DEFAULT '',
            executed_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS session_tags (
            session_id TEXT NOT NULL,
            tag TEXT NOT NULL,
            PRIMARY KEY (session_id, tag)
        );

        CREATE TABLE IF NOT EXISTS session_fingerprints (
            session_id TEXT PRIMARY KEY,
            services_seen TEXT NOT NULL DEFAULT '',
            technologies_seen TEXT NOT NULL DEFAULT '',
            vuln_classes_found TEXT NOT NULL DEFAULT '',
            flags_captured INTEGER NOT NULL DEFAULT 0,
            hosts_count INTEGER NOT NULL DEFAULT 0,
            summary TEXT NOT NULL DEFAULT '',
            outcome TEXT NOT NULL DEFAULT '',
            goal_type TEXT NOT NULL DEFAULT ''
        );",
    ),
];

/// SQLite-backed persistent storage for scan sessions.
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open (or create) the database at `~/.redtrail/redtrail.db`.
    pub fn open() -> Result<Self, Error> {
        let dir = dirs_path()?;
        std::fs::create_dir_all(&dir)
            .map_err(|e| Error::Db(format!("failed to create ~/.redtrail directory: {e}")))?;
        let db_path = dir.join("redtrail.db");
        let conn = Connection::open(&db_path)
            .map_err(|e| Error::Db(format!("failed to open database: {e}")))?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| Error::Db(format!("failed to set WAL mode: {e}")))?;

        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Open an in-memory database (for testing).
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, Error> {
        let conn = Connection::open_in_memory()
            .map_err(|e| Error::Db(format!("failed to open in-memory db: {e}")))?;

        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Run all pending schema migrations sequentially.
    /// Creates the `schema_version` table on first run, then applies
    /// only migrations newer than the current version.
    fn migrate(&self) -> Result<(), Error> {
        // Ensure the version-tracking table exists (idempotent).
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_version (
                    version INTEGER NOT NULL
                );",
            )
            .map_err(|e| Error::Db(format!("failed to create schema_version table: {e}")))?;

        let current_version: Option<u32> = self
            .conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .map_err(|e| Error::Db(format!("failed to read schema version: {e}")))?;

        for &(version, sql) in MIGRATIONS {
            if current_version.is_some_and(|v| v >= version) {
                continue;
            }
            self.conn
                .execute_batch(sql)
                .map_err(|e| Error::Db(format!("migration v{version} failed: {e}")))?;
            self.conn
                .execute(
                    "INSERT INTO schema_version (version) VALUES (?1)",
                    params![version],
                )
                .map_err(|e| Error::Db(format!("failed to record migration v{version}: {e}")))?;
        }

        Ok(())
    }

    /// Return the current schema version, or `None` if no migrations have run.
    #[cfg(test)]
    fn schema_version(&self) -> Result<Option<u32>, Error> {
        self.conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .map_err(|e| Error::Db(format!("failed to read schema version: {e}")))
    }

    /// Save a session, returning its ID. Upserts on conflict.
    pub fn save_session(&self, session: &ScanSession) -> Result<String, Error> {
        let target_hosts_json = serde_json::to_string(&session.target_hosts)
            .map_err(|e| Error::Db(format!("failed to serialize target_hosts: {e}")))?;
        let knowledge_json = serde_json::to_string(&session.knowledge)
            .map_err(|e| Error::Db(format!("failed to serialize knowledge: {e}")))?;
        let findings_json = serde_json::to_string(&session.findings)
            .map_err(|e| Error::Db(format!("failed to serialize findings: {e}")))?;
        let status_str = status_to_str(&session.status);

        self.conn
            .execute(
                "INSERT INTO sessions (id, created_at, updated_at, target_url, target_hosts,
                    total_turns_used, max_turns_configured, llm_provider, knowledge, findings, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(id) DO UPDATE SET
                    updated_at = excluded.updated_at,
                    total_turns_used = excluded.total_turns_used,
                    knowledge = excluded.knowledge,
                    findings = excluded.findings,
                    status = excluded.status",
                params![
                    session.id,
                    session.created_at,
                    session.updated_at,
                    session.target_url,
                    target_hosts_json,
                    session.total_turns_used,
                    session.max_turns_configured,
                    session.llm_provider,
                    knowledge_json,
                    findings_json,
                    status_str,
                ],
            )
            .map_err(|e| Error::Db(format!("failed to save session: {e}")))?;

        Ok(session.id.clone())
    }

    /// Load a session by ID.
    pub fn load_session(&self, id: &str) -> Result<ScanSession, Error> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, created_at, updated_at, target_url, target_hosts,
                    total_turns_used, max_turns_configured, llm_provider,
                    knowledge, findings, status
                 FROM sessions WHERE id = ?1",
            )
            .map_err(|e| Error::Db(format!("failed to prepare query: {e}")))?;

        stmt.query_row(params![id], |row| Ok(row_to_session(row)))
            .map_err(|e| Error::Db(format!("session not found: {e}")))?
            .map_err(|e| Error::Db(format!("failed to deserialize session: {e}")))
    }

    /// Load the most recent session by updated_at.
    pub fn latest_session(&self) -> Result<Option<ScanSession>, Error> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, created_at, updated_at, target_url, target_hosts,
                    total_turns_used, max_turns_configured, llm_provider,
                    knowledge, findings, status
                 FROM sessions ORDER BY updated_at DESC LIMIT 1",
            )
            .map_err(|e| Error::Db(format!("failed to prepare query: {e}")))?;

        let result = stmt
            .query_row([], |row| Ok(row_to_session(row)))
            .optional()
            .map_err(|e| Error::Db(format!("failed to query latest session: {e}")))?;

        match result {
            Some(inner) => inner
                .map(Some)
                .map_err(|e| Error::Db(format!("failed to deserialize session: {e}"))),
            None => Ok(None),
        }
    }

    /// Upsert an attack pattern. If a pattern with the same technique + service_type
    /// exists, update stats (increment attempts/successes, recalculate averages).
    /// If new, insert with initial stats.
    pub fn upsert_attack_pattern(&self, pattern: &AttackPattern) -> Result<(), Error> {
        // Check if a pattern with the same technique + service_type already exists.
        let existing: Option<(i64, u32, u32, f64, f64)> = self
            .conn
            .query_row(
                "SELECT id, total_attempts, successes, avg_tool_calls, avg_duration_secs
                 FROM attack_patterns WHERE technique = ?1 AND service_type = ?2",
                params![pattern.technique, pattern.service_type],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| Error::Db(format!("failed to query attack_patterns: {e}")))?;

        match existing {
            Some((id, old_attempts, old_successes, old_avg_calls, old_avg_duration)) => {
                let new_attempts = old_attempts + pattern.total_attempts;
                let new_successes = old_successes + pattern.successes;
                // Running weighted average: (old_avg * old_n + new_avg * new_n) / total_n
                let new_avg_calls = if new_attempts > 0 {
                    (old_avg_calls * old_attempts as f64
                        + pattern.avg_tool_calls * pattern.total_attempts as f64)
                        / new_attempts as f64
                } else {
                    0.0
                };
                let new_avg_duration = if new_attempts > 0 {
                    (old_avg_duration * old_attempts as f64
                        + pattern.avg_duration_secs * pattern.total_attempts as f64)
                        / new_attempts as f64
                } else {
                    0.0
                };

                self.conn
                    .execute(
                        "UPDATE attack_patterns SET
                            total_attempts = ?1,
                            successes = ?2,
                            avg_tool_calls = ?3,
                            avg_duration_secs = ?4,
                            brute_force_needed = ?5,
                            attack_chain = ?6,
                            last_seen_at = ?7,
                            last_session_id = ?8,
                            vulnerability_class = ?9,
                            technology_stack = ?10
                         WHERE id = ?11",
                        params![
                            new_attempts,
                            new_successes,
                            new_avg_calls,
                            new_avg_duration,
                            pattern.brute_force_needed as i32,
                            pattern.attack_chain,
                            pattern.last_seen_at,
                            pattern.last_session_id,
                            pattern.vulnerability_class,
                            pattern.technology_stack,
                            id,
                        ],
                    )
                    .map_err(|e| Error::Db(format!("failed to update attack_pattern: {e}")))?;
            }
            None => {
                self.conn
                    .execute(
                        "INSERT INTO attack_patterns (
                            technique, vulnerability_class, service_type, technology_stack,
                            total_attempts, successes, avg_tool_calls, avg_duration_secs,
                            brute_force_needed, attack_chain, first_seen_at, last_seen_at,
                            last_session_id
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                        params![
                            pattern.technique,
                            pattern.vulnerability_class,
                            pattern.service_type,
                            pattern.technology_stack,
                            pattern.total_attempts,
                            pattern.successes,
                            pattern.avg_tool_calls,
                            pattern.avg_duration_secs,
                            pattern.brute_force_needed as i32,
                            pattern.attack_chain,
                            pattern.first_seen_at,
                            pattern.last_seen_at,
                            pattern.last_session_id,
                        ],
                    )
                    .map_err(|e| Error::Db(format!("failed to insert attack_pattern: {e}")))?;
            }
        }

        Ok(())
    }

    /// Record a technique execution, inserting a row into `technique_executions`.
    pub fn record_execution(&self, exec: &TechniqueExecution) -> Result<(), Error> {
        self.conn
            .execute(
                "INSERT INTO technique_executions (
                    session_id, task_type, target_host, target_service,
                    tool_calls, wall_clock_secs, succeeded, brute_force_used,
                    technology_stack, executed_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    exec.session_id,
                    exec.task_type,
                    exec.target_host,
                    exec.target_service,
                    exec.tool_calls,
                    exec.wall_clock_secs,
                    exec.succeeded as i32,
                    exec.brute_force_used as i32,
                    exec.technology_stack,
                    exec.executed_at,
                ],
            )
            .map_err(|e| Error::Db(format!("failed to insert technique_execution: {e}")))?;

        Ok(())
    }

    /// Save a session fingerprint, upserting on session_id conflict (INSERT OR REPLACE).
    pub fn save_fingerprint(&self, fp: &SessionFingerprint) -> Result<(), Error> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO session_fingerprints (
                    session_id, services_seen, technologies_seen, vuln_classes_found,
                    flags_captured, hosts_count, summary, outcome, goal_type
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    fp.session_id,
                    fp.services_seen,
                    fp.technologies_seen,
                    fp.vuln_classes_found,
                    fp.flags_captured,
                    fp.hosts_count,
                    fp.summary,
                    fp.outcome,
                    fp.goal_type,
                ],
            )
            .map_err(|e| Error::Db(format!("failed to save session_fingerprint: {e}")))?;

        Ok(())
    }

    /// Tag a session. Ignores duplicate tags (INSERT OR IGNORE).
    pub fn tag_session(&self, session_id: &str, tag: &str) -> Result<(), Error> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO session_tags (session_id, tag) VALUES (?1, ?2)",
                params![session_id, tag],
            )
            .map_err(|e| Error::Db(format!("failed to tag session: {e}")))?;

        Ok(())
    }

    /// Return all session IDs that have a given tag.
    pub fn sessions_by_tag(&self, tag: &str) -> Result<Vec<String>, Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT session_id FROM session_tags WHERE tag = ?1")
            .map_err(|e| Error::Db(format!("failed to prepare sessions_by_tag query: {e}")))?;

        let rows = stmt
            .query_map(params![tag], |row| row.get(0))
            .map_err(|e| Error::Db(format!("failed to query sessions_by_tag: {e}")))?;

        let mut ids = Vec::new();
        for row in rows {
            ids.push(row.map_err(|e| Error::Db(format!("failed to read row: {e}")))?);
        }
        Ok(ids)
    }

    /// Gather cross-session intelligence matching the given query.
    ///
    /// Returns attack patterns matching services/technologies, similar session
    /// fingerprints, and aggregated technique stats. Results are capped to keep
    /// the serialized payload under ~1500 tokens.
    pub fn gather_cross_session_intel(
        &self,
        query: &RelevanceQuery,
    ) -> Result<CrossSessionIntel, Error> {
        // 1. Query attack_patterns matching services or technologies
        let relevant_patterns = self.query_attack_patterns(query)?;

        // 2. Query session_fingerprints for similar sessions
        let similar_sessions = self.find_similar_sessions(query, 5)?;

        // 3. Compute technique stats from technique_executions
        let technique_stats = self.compute_technique_stats(query)?;

        Ok(CrossSessionIntel {
            relevant_patterns,
            similar_sessions,
            technique_stats,
        })
    }

    /// Find sessions similar to the current one based on overlapping services
    /// and technologies. Returns up to `limit` results, ranked by similarity.
    pub fn find_similar_sessions(
        &self,
        query: &RelevanceQuery,
        limit: usize,
    ) -> Result<Vec<SessionFingerprint>, Error> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT session_id, services_seen, technologies_seen, vuln_classes_found,
                        flags_captured, hosts_count, summary, outcome, goal_type
                 FROM session_fingerprints",
            )
            .map_err(|e| Error::Db(format!("failed to prepare find_similar_sessions: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(SessionFingerprint {
                    session_id: row.get(0)?,
                    services_seen: row.get(1)?,
                    technologies_seen: row.get(2)?,
                    vuln_classes_found: row.get(3)?,
                    flags_captured: row.get(4)?,
                    hosts_count: row.get(5)?,
                    summary: row.get(6)?,
                    outcome: row.get(7)?,
                    goal_type: row.get(8)?,
                })
            })
            .map_err(|e| Error::Db(format!("failed to query session_fingerprints: {e}")))?;

        let mut scored: Vec<(usize, SessionFingerprint)> = Vec::new();
        for row in rows {
            let fp =
                row.map_err(|e| Error::Db(format!("failed to read fingerprint row: {e}")))?;
            let score = Self::similarity_score(&fp, query);
            if score > 0 {
                scored.push((score, fp));
            }
        }

        // Sort descending by score
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(_, fp)| fp).collect())
    }

    /// Score similarity between a session fingerprint and a relevance query.
    /// Returns the number of overlapping services + technologies (+ goal_type bonus).
    fn similarity_score(fp: &SessionFingerprint, query: &RelevanceQuery) -> usize {
        let fp_services: Vec<&str> = fp
            .services_seen
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        let fp_techs: Vec<&str> = fp
            .technologies_seen
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        let mut score = 0usize;
        for svc in &query.services {
            if fp_services.iter().any(|s| s.eq_ignore_ascii_case(svc)) {
                score += 1;
            }
        }
        for tech in &query.technologies {
            if fp_techs.iter().any(|t| t.eq_ignore_ascii_case(tech)) {
                score += 1;
            }
        }
        // Bonus for matching goal type
        if let Some(ref gt) = query.goal_type
            && fp.goal_type.eq_ignore_ascii_case(gt)
        {
            score += 1;
        }
        score
    }

    /// Query attack patterns matching any of the query's services or technologies.
    /// Returns up to 10 patterns ordered by success rate.
    fn query_attack_patterns(
        &self,
        query: &RelevanceQuery,
    ) -> Result<Vec<AttackPattern>, Error> {
        // Build WHERE clause with LIKE conditions for services and technologies
        let mut conditions = Vec::new();
        let mut param_values: Vec<String> = Vec::new();

        for svc in &query.services {
            param_values.push(format!("%{svc}%"));
            conditions.push(format!("service_type LIKE ?{}", param_values.len()));
        }
        for tech in &query.technologies {
            param_values.push(format!("%{tech}%"));
            conditions.push(format!("technology_stack LIKE ?{}", param_values.len()));
        }

        if conditions.is_empty() {
            return Ok(Vec::new());
        }

        let where_clause = conditions.join(" OR ");
        let sql = format!(
            "SELECT id, technique, vulnerability_class, service_type, technology_stack,
                    total_attempts, successes, avg_tool_calls, avg_duration_secs,
                    brute_force_needed, attack_chain, first_seen_at, last_seen_at,
                    last_session_id
             FROM attack_patterns
             WHERE {where_clause}
             ORDER BY CASE WHEN total_attempts > 0 THEN CAST(successes AS REAL) / total_attempts ELSE 0 END DESC
             LIMIT 10"
        );

        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| Error::Db(format!("failed to prepare attack_patterns query: {e}")))?;

        let params_refs: Vec<&dyn rusqlite::ToSql> = param_values
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect();

        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(AttackPattern {
                    id: row.get(0)?,
                    technique: row.get(1)?,
                    vulnerability_class: row.get(2)?,
                    service_type: row.get(3)?,
                    technology_stack: row.get(4)?,
                    total_attempts: row.get(5)?,
                    successes: row.get(6)?,
                    avg_tool_calls: row.get(7)?,
                    avg_duration_secs: row.get(8)?,
                    brute_force_needed: row.get::<_, i32>(9).map(|v| v != 0)?,
                    attack_chain: row.get(10)?,
                    first_seen_at: row.get(11)?,
                    last_seen_at: row.get(12)?,
                    last_session_id: row.get(13)?,
                })
            })
            .map_err(|e| Error::Db(format!("failed to query attack_patterns: {e}")))?;

        let mut patterns = Vec::new();
        for row in rows {
            patterns.push(
                row.map_err(|e| Error::Db(format!("failed to read attack_pattern row: {e}")))?,
            );
        }
        Ok(patterns)
    }

    /// Compute aggregated technique stats from technique_executions matching
    /// the query's services or technologies.
    fn compute_technique_stats(
        &self,
        query: &RelevanceQuery,
    ) -> Result<Vec<TechniqueStats>, Error> {
        let mut conditions = Vec::new();
        let mut param_values: Vec<String> = Vec::new();

        for svc in &query.services {
            param_values.push(format!("%{svc}%"));
            conditions.push(format!("target_service LIKE ?{}", param_values.len()));
        }
        for tech in &query.technologies {
            param_values.push(format!("%{tech}%"));
            conditions.push(format!("technology_stack LIKE ?{}", param_values.len()));
        }

        if conditions.is_empty() {
            return Ok(Vec::new());
        }

        let where_clause = conditions.join(" OR ");
        let sql = format!(
            "SELECT task_type,
                    AVG(tool_calls) as avg_tool_calls,
                    AVG(CASE WHEN succeeded THEN 1.0 ELSE 0.0 END) as success_rate,
                    AVG(wall_clock_secs) as avg_duration
             FROM technique_executions
             WHERE {where_clause}
             GROUP BY task_type
             ORDER BY success_rate DESC
             LIMIT 10"
        );

        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| Error::Db(format!("failed to prepare technique_stats query: {e}")))?;

        let params_refs: Vec<&dyn rusqlite::ToSql> = param_values
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect();

        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(TechniqueStats {
                    task_type: row.get(0)?,
                    avg_tool_calls: row.get(1)?,
                    success_rate: row.get(2)?,
                    avg_duration: row.get(3)?,
                })
            })
            .map_err(|e| Error::Db(format!("failed to query technique_stats: {e}")))?;

        let mut stats = Vec::new();
        for row in rows {
            stats.push(
                row.map_err(|e| Error::Db(format!("failed to read technique_stats row: {e}")))?,
            );
        }
        Ok(stats)
    }

    /// List all sessions as summaries.
    pub fn list_sessions(&self) -> Result<Vec<SessionSummary>, Error> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, created_at, target_url, total_turns_used, findings, status
                 FROM sessions ORDER BY updated_at DESC",
            )
            .map_err(|e| Error::Db(format!("failed to prepare query: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let findings_json: String = row.get(4)?;
                let findings_count = serde_json::from_str::<Vec<serde_json::Value>>(&findings_json)
                    .map(|v| v.len())
                    .unwrap_or(0);
                let status_str: String = row.get(5)?;

                Ok(SessionSummary {
                    id: row.get(0)?,
                    created_at: row.get(1)?,
                    target_url: row.get(2)?,
                    total_turns_used: row.get(3)?,
                    findings_count,
                    status: str_to_status(&status_str),
                })
            })
            .map_err(|e| Error::Db(format!("failed to list sessions: {e}")))?;

        let mut summaries = Vec::new();
        for row in rows {
            summaries.push(row.map_err(|e| Error::Db(format!("failed to read row: {e}")))?);
        }
        Ok(summaries)
    }
}

fn row_to_session(row: &rusqlite::Row<'_>) -> Result<ScanSession, String> {
    let id: String = row.get(0).map_err(|e| e.to_string())?;
    let created_at: String = row.get(1).map_err(|e| e.to_string())?;
    let updated_at: String = row.get(2).map_err(|e| e.to_string())?;
    let target_url: Option<String> = row.get(3).map_err(|e| e.to_string())?;
    let target_hosts_json: String = row.get(4).map_err(|e| e.to_string())?;
    let total_turns_used: u32 = row.get(5).map_err(|e| e.to_string())?;
    let max_turns_configured: u32 = row.get(6).map_err(|e| e.to_string())?;
    let llm_provider: String = row.get(7).map_err(|e| e.to_string())?;
    let knowledge_json: String = row.get(8).map_err(|e| e.to_string())?;
    let findings_json: String = row.get(9).map_err(|e| e.to_string())?;
    let status_str: String = row.get(10).map_err(|e| e.to_string())?;

    let target_hosts: Vec<String> =
        serde_json::from_str(&target_hosts_json).map_err(|e| e.to_string())?;
    let knowledge = serde_json::from_str(&knowledge_json).map_err(|e| e.to_string())?;
    let findings = serde_json::from_str(&findings_json).map_err(|e| e.to_string())?;
    let status = str_to_status(&status_str);

    Ok(ScanSession {
        id,
        created_at,
        updated_at,
        target_url,
        target_hosts,
        total_turns_used,
        max_turns_configured,
        llm_provider,
        knowledge,
        findings,
        status,
    })
}

fn status_to_str(status: &SessionStatus) -> &'static str {
    match status {
        SessionStatus::Running => "running",
        SessionStatus::Completed => "completed",
        SessionStatus::Interrupted => "interrupted",
    }
}

fn str_to_status(s: &str) -> SessionStatus {
    match s {
        "running" => SessionStatus::Running,
        "interrupted" => SessionStatus::Interrupted,
        _ => SessionStatus::Completed,
    }
}

fn dirs_path() -> Result<std::path::PathBuf, Error> {
    let home = std::env::var("HOME")
        .map_err(|_| Error::Db("HOME environment variable not set".into()))?;
    Ok(std::path::PathBuf::from(home).join(".redtrail"))
}

/// Extension trait for rusqlite to add `optional()` to query results.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::knowledge::KnowledgeBase;

    fn make_test_session(id: &str) -> ScanSession {
        ScanSession {
            id: id.to_string(),
            created_at: "2026-03-05T10:00:00Z".to_string(),
            updated_at: "2026-03-05T10:05:00Z".to_string(),
            target_url: Some("http://test.local".to_string()),
            target_hosts: vec!["10.0.0.1".to_string()],
            total_turns_used: 42,
            max_turns_configured: 500,
            llm_provider: "claude".to_string(),
            knowledge: KnowledgeBase::new(),
            findings: vec![],
            status: SessionStatus::Completed,
        }
    }

    #[test]
    fn test_save_and_load_session() {
        let db = Db::open_in_memory().unwrap();
        let session = make_test_session("test-id-1");

        let id = db.save_session(&session).unwrap();
        assert_eq!(id, "test-id-1");

        let loaded = db.load_session("test-id-1").unwrap();
        assert_eq!(loaded.id, "test-id-1");
        assert_eq!(loaded.target_url, Some("http://test.local".to_string()));
        assert_eq!(loaded.total_turns_used, 42);
        assert_eq!(loaded.status, SessionStatus::Completed);
    }

    #[test]
    fn test_latest_session() {
        let db = Db::open_in_memory().unwrap();

        // No sessions yet
        assert!(db.latest_session().unwrap().is_none());

        let mut s1 = make_test_session("s1");
        s1.updated_at = "2026-03-05T10:00:00Z".to_string();
        db.save_session(&s1).unwrap();

        let mut s2 = make_test_session("s2");
        s2.updated_at = "2026-03-05T11:00:00Z".to_string();
        db.save_session(&s2).unwrap();

        let latest = db.latest_session().unwrap().unwrap();
        assert_eq!(latest.id, "s2");
    }

    #[test]
    fn test_list_sessions() {
        let db = Db::open_in_memory().unwrap();
        db.save_session(&make_test_session("a")).unwrap();
        db.save_session(&make_test_session("b")).unwrap();

        let list = db.list_sessions().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_upsert_session() {
        let db = Db::open_in_memory().unwrap();
        let mut session = make_test_session("upsert-test");
        session.total_turns_used = 10;
        db.save_session(&session).unwrap();

        session.total_turns_used = 50;
        session.updated_at = "2026-03-05T12:00:00Z".to_string();
        db.save_session(&session).unwrap();

        let loaded = db.load_session("upsert-test").unwrap();
        assert_eq!(loaded.total_turns_used, 50);

        // Should still be only 1 session
        let list = db.list_sessions().unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_load_nonexistent_session() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.load_session("nonexistent").is_err());
    }

    #[test]
    fn test_migration_runs_on_fresh_db() {
        let db = Db::open_in_memory().unwrap();
        // schema_version table exists and records all migrations
        let version = db.schema_version().unwrap();
        assert_eq!(version, Some(1));
        // sessions table was created by migration v0
        let count: u32 = db
            .conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_migration_idempotent_on_reopen() {
        // Simulate opening twice by calling migrate() again on the same connection.
        let db = Db::open_in_memory().unwrap();
        let v1 = db.schema_version().unwrap();
        assert_eq!(v1, Some(1));

        // Run migrate again — should be a no-op.
        db.migrate().unwrap();
        let v2 = db.schema_version().unwrap();
        assert_eq!(v2, Some(1));

        // Two version rows should exist (v0 and v1).
        let row_count: u32 = db
            .conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(row_count, 2);
    }

    #[test]
    fn test_v1_migration_creates_cross_session_tables() {
        let db = Db::open_in_memory().unwrap();

        // Schema version should be 1 after all migrations
        let version = db.schema_version().unwrap();
        assert_eq!(version, Some(1));

        // Verify all four tables exist by querying them
        let tables = [
            "attack_patterns",
            "technique_executions",
            "session_tags",
            "session_fingerprints",
        ];
        for table in &tables {
            let count: u32 = db
                .conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap_or_else(|e| panic!("table '{table}' should exist: {e}"));
            assert_eq!(count, 0, "table '{table}' should be empty");
        }
    }

    #[test]
    fn test_cross_session_structs_serialize_roundtrip() {
        let pattern = AttackPattern {
            id: 0,
            technique: "sqli_union".into(),
            vulnerability_class: "sql_injection".into(),
            service_type: "http".into(),
            technology_stack: "php,mysql".into(),
            total_attempts: 5,
            successes: 3,
            avg_tool_calls: 4.2,
            avg_duration_secs: 12.5,
            brute_force_needed: false,
            attack_chain: r#"["recon","probe","exploit"]"#.into(),
            first_seen_at: "2026-03-01T00:00:00Z".into(),
            last_seen_at: "2026-03-09T00:00:00Z".into(),
            last_session_id: "sess-1".into(),
        };
        let json = serde_json::to_string(&pattern).unwrap();
        let rt: AttackPattern = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.technique, "sqli_union");
        assert_eq!(rt.successes, 3);
        assert!(!rt.brute_force_needed);

        let exec = TechniqueExecution {
            id: 0,
            session_id: "sess-1".into(),
            task_type: "differential_probe".into(),
            target_host: "10.0.0.1".into(),
            target_service: "http".into(),
            tool_calls: 3,
            wall_clock_secs: 5.2,
            succeeded: true,
            brute_force_used: false,
            technology_stack: "nginx".into(),
            executed_at: "2026-03-09T10:00:00Z".into(),
        };
        let json = serde_json::to_string(&exec).unwrap();
        let rt: TechniqueExecution = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.task_type, "differential_probe");
        assert!(rt.succeeded);

        let fp = SessionFingerprint {
            session_id: "sess-1".into(),
            services_seen: "http,ssh".into(),
            technologies_seen: "nginx,php".into(),
            vuln_classes_found: "sqli".into(),
            flags_captured: 2,
            hosts_count: 3,
            summary: "web pentest".into(),
            outcome: "achieved".into(),
            goal_type: "capture-flags".into(),
        };
        let json = serde_json::to_string(&fp).unwrap();
        let rt: SessionFingerprint = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.flags_captured, 2);

        let intel = CrossSessionIntel {
            relevant_patterns: vec![pattern],
            similar_sessions: vec![fp],
            technique_stats: vec![TechniqueStats {
                task_type: "differential_probe".into(),
                avg_tool_calls: 3.0,
                success_rate: 0.8,
                avg_duration: 5.0,
            }],
        };
        let json = serde_json::to_string(&intel).unwrap();
        let rt: CrossSessionIntel = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.relevant_patterns.len(), 1);
        assert_eq!(rt.technique_stats[0].success_rate, 0.8);

        let query = RelevanceQuery {
            services: vec!["http".into()],
            technologies: vec!["nginx".into()],
            goal_type: Some("capture-flags".into()),
            tags: vec!["ctf".into()],
        };
        let json = serde_json::to_string(&query).unwrap();
        let rt: RelevanceQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.services, vec!["http"]);
        assert_eq!(rt.goal_type, Some("capture-flags".into()));
    }

    #[test]
    fn test_cross_session_structs_deserialize_with_defaults() {
        // Minimal JSON should deserialize with defaults for optional/defaulted fields
        let json = r#"{"technique":"test","vulnerability_class":"xss","service_type":"http"}"#;
        let pattern: AttackPattern = serde_json::from_str(json).unwrap();
        assert_eq!(pattern.id, 0);
        assert_eq!(pattern.total_attempts, 0);
        assert_eq!(pattern.avg_tool_calls, 0.0);
        assert!(!pattern.brute_force_needed);
        assert!(pattern.technology_stack.is_empty());

        let json = r#"{"session_id":"s1","task_type":"recon","target_host":"10.0.0.1"}"#;
        let exec: TechniqueExecution = serde_json::from_str(json).unwrap();
        assert_eq!(exec.tool_calls, 0);
        assert!(!exec.succeeded);

        let json = r#"{"session_id":"s1"}"#;
        let fp: SessionFingerprint = serde_json::from_str(json).unwrap();
        assert_eq!(fp.flags_captured, 0);
        assert!(fp.services_seen.is_empty());

        let json = r#"{}"#;
        let query: RelevanceQuery = serde_json::from_str(json).unwrap();
        assert!(query.services.is_empty());
        assert!(query.goal_type.is_none());
    }

    #[test]
    fn test_upsert_attack_pattern_insert_then_update() {
        let db = Db::open_in_memory().unwrap();

        // Insert a new pattern
        let pattern = AttackPattern {
            id: 0,
            technique: "sqli_union".into(),
            vulnerability_class: "sql_injection".into(),
            service_type: "http".into(),
            technology_stack: "php,mysql".into(),
            total_attempts: 3,
            successes: 2,
            avg_tool_calls: 5.0,
            avg_duration_secs: 10.0,
            brute_force_needed: false,
            attack_chain: r#"["recon","probe","exploit"]"#.into(),
            first_seen_at: "2026-03-01T00:00:00Z".into(),
            last_seen_at: "2026-03-05T00:00:00Z".into(),
            last_session_id: "sess-1".into(),
        };
        db.upsert_attack_pattern(&pattern).unwrap();

        // Verify inserted
        let row: (u32, u32, f64, f64, String) = db
            .conn
            .query_row(
                "SELECT total_attempts, successes, avg_tool_calls, avg_duration_secs, last_session_id
                 FROM attack_patterns WHERE technique = 'sqli_union' AND service_type = 'http'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .unwrap();
        assert_eq!(row.0, 3); // total_attempts
        assert_eq!(row.1, 2); // successes
        assert!((row.2 - 5.0).abs() < 0.01); // avg_tool_calls
        assert!((row.3 - 10.0).abs() < 0.01); // avg_duration_secs
        assert_eq!(row.4, "sess-1");

        // Upsert with new stats (same technique + service_type)
        let pattern2 = AttackPattern {
            id: 0,
            technique: "sqli_union".into(),
            vulnerability_class: "sql_injection".into(),
            service_type: "http".into(),
            technology_stack: "php,mysql".into(),
            total_attempts: 2,
            successes: 1,
            avg_tool_calls: 3.0,
            avg_duration_secs: 8.0,
            brute_force_needed: false,
            attack_chain: r#"["recon","exploit"]"#.into(),
            first_seen_at: "2026-03-05T00:00:00Z".into(),
            last_seen_at: "2026-03-09T00:00:00Z".into(),
            last_session_id: "sess-2".into(),
        };
        db.upsert_attack_pattern(&pattern2).unwrap();

        // Verify aggregation
        let row: (u32, u32, f64, f64, String, String) = db
            .conn
            .query_row(
                "SELECT total_attempts, successes, avg_tool_calls, avg_duration_secs,
                        last_session_id, first_seen_at
                 FROM attack_patterns WHERE technique = 'sqli_union' AND service_type = 'http'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row.0, 5); // 3 + 2 total_attempts
        assert_eq!(row.1, 3); // 2 + 1 successes
        // Weighted avg: (5.0*3 + 3.0*2) / 5 = 21/5 = 4.2
        assert!((row.2 - 4.2).abs() < 0.01);
        // Weighted avg: (10.0*3 + 8.0*2) / 5 = 46/5 = 9.2
        assert!((row.3 - 9.2).abs() < 0.01);
        assert_eq!(row.4, "sess-2"); // updated to latest
        assert_eq!(row.5, "2026-03-01T00:00:00Z"); // first_seen_at preserved

        // Should still be only 1 row
        let count: u32 = db
            .conn
            .query_row("SELECT COUNT(*) FROM attack_patterns", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);

        // Different service_type should create a new row
        let pattern3 = AttackPattern {
            id: 0,
            technique: "sqli_union".into(),
            vulnerability_class: "sql_injection".into(),
            service_type: "https".into(),
            technology_stack: "java".into(),
            total_attempts: 1,
            successes: 0,
            avg_tool_calls: 7.0,
            avg_duration_secs: 15.0,
            brute_force_needed: true,
            attack_chain: r#"["recon"]"#.into(),
            first_seen_at: "2026-03-09T00:00:00Z".into(),
            last_seen_at: "2026-03-09T00:00:00Z".into(),
            last_session_id: "sess-3".into(),
        };
        db.upsert_attack_pattern(&pattern3).unwrap();

        let count: u32 = db
            .conn
            .query_row("SELECT COUNT(*) FROM attack_patterns", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_record_execution_insert_and_retrieve() {
        let db = Db::open_in_memory().unwrap();

        let exec = TechniqueExecution {
            id: 0,
            session_id: "sess-1".into(),
            task_type: "differential_probe".into(),
            target_host: "10.0.0.1".into(),
            target_service: "http".into(),
            tool_calls: 3,
            wall_clock_secs: 5.2,
            succeeded: true,
            brute_force_used: false,
            technology_stack: "nginx".into(),
            executed_at: "2026-03-09T10:00:00Z".into(),
        };
        db.record_execution(&exec).unwrap();

        // Verify retrieval
        let row: (
            String,
            String,
            String,
            String,
            u32,
            f64,
            i32,
            i32,
            String,
            String,
        ) = db
            .conn
            .query_row(
                "SELECT session_id, task_type, target_host, target_service,
                        tool_calls, wall_clock_secs, succeeded, brute_force_used,
                        technology_stack, executed_at
                 FROM technique_executions WHERE session_id = 'sess-1'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row.0, "sess-1");
        assert_eq!(row.1, "differential_probe");
        assert_eq!(row.2, "10.0.0.1");
        assert_eq!(row.3, "http");
        assert_eq!(row.4, 3);
        assert!((row.5 - 5.2).abs() < 0.01);
        assert_eq!(row.6, 1); // succeeded = true
        assert_eq!(row.7, 0); // brute_force_used = false
        assert_eq!(row.8, "nginx");
        assert_eq!(row.9, "2026-03-09T10:00:00Z");

        // Insert another execution for same session
        let exec2 = TechniqueExecution {
            id: 0,
            session_id: "sess-1".into(),
            task_type: "stack_fingerprint".into(),
            target_host: "10.0.0.1".into(),
            target_service: "ssh".into(),
            tool_calls: 2,
            wall_clock_secs: 3.1,
            succeeded: false,
            brute_force_used: false,
            technology_stack: "openssh".into(),
            executed_at: "2026-03-09T10:05:00Z".into(),
        };
        db.record_execution(&exec2).unwrap();

        // Verify both rows exist
        let count: u32 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM technique_executions WHERE session_id = 'sess-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_save_and_load_fingerprint() {
        let db = Db::open_in_memory().unwrap();

        let fp = SessionFingerprint {
            session_id: "sess-fp-1".into(),
            services_seen: "http,ssh,ftp".into(),
            technologies_seen: "nginx,php,mysql".into(),
            vuln_classes_found: "sqli,xss".into(),
            flags_captured: 3,
            hosts_count: 2,
            summary: "Web pentest with SQL injection".into(),
            outcome: "achieved".into(),
            goal_type: "capture-flags".into(),
        };
        db.save_fingerprint(&fp).unwrap();

        // Verify retrieval
        let row: (String, String, String, u32, u32, String, String, String) = db
            .conn
            .query_row(
                "SELECT services_seen, technologies_seen, vuln_classes_found,
                        flags_captured, hosts_count, summary, outcome, goal_type
                 FROM session_fingerprints WHERE session_id = 'sess-fp-1'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row.0, "http,ssh,ftp");
        assert_eq!(row.1, "nginx,php,mysql");
        assert_eq!(row.2, "sqli,xss");
        assert_eq!(row.3, 3);
        assert_eq!(row.4, 2);
        assert_eq!(row.5, "Web pentest with SQL injection");
        assert_eq!(row.6, "achieved");
        assert_eq!(row.7, "capture-flags");

        // Upsert: update the same session_id with new data
        let fp2 = SessionFingerprint {
            session_id: "sess-fp-1".into(),
            services_seen: "http,ssh,ftp,smb".into(),
            technologies_seen: "nginx,php,mysql,samba".into(),
            vuln_classes_found: "sqli,xss,lfi".into(),
            flags_captured: 5,
            hosts_count: 4,
            summary: "Updated pentest summary".into(),
            outcome: "achieved".into(),
            goal_type: "capture-flags".into(),
        };
        db.save_fingerprint(&fp2).unwrap();

        // Verify upsert replaced the row
        let count: u32 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM session_fingerprints WHERE session_id = 'sess-fp-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let flags: u32 = db
            .conn
            .query_row(
                "SELECT flags_captured FROM session_fingerprints WHERE session_id = 'sess-fp-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(flags, 5); // updated value
    }

    #[test]
    fn test_tag_session_and_sessions_by_tag() {
        let db = Db::open_in_memory().unwrap();

        // Tag two sessions with "ctf"
        db.tag_session("sess-1", "ctf").unwrap();
        db.tag_session("sess-2", "ctf").unwrap();
        // Tag one with "htb"
        db.tag_session("sess-1", "htb").unwrap();

        // Duplicate tag should be ignored (no error)
        db.tag_session("sess-1", "ctf").unwrap();

        // Query by "ctf" tag
        let ctf_sessions = db.sessions_by_tag("ctf").unwrap();
        assert_eq!(ctf_sessions.len(), 2);
        assert!(ctf_sessions.contains(&"sess-1".to_string()));
        assert!(ctf_sessions.contains(&"sess-2".to_string()));

        // Query by "htb" tag
        let htb_sessions = db.sessions_by_tag("htb").unwrap();
        assert_eq!(htb_sessions.len(), 1);
        assert_eq!(htb_sessions[0], "sess-1");

        // Query by nonexistent tag
        let empty = db.sessions_by_tag("nonexistent").unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_gather_cross_session_intel() {
        let db = Db::open_in_memory().unwrap();

        // Seed attack patterns
        db.upsert_attack_pattern(&AttackPattern {
            id: 0,
            technique: "sqli_union".into(),
            vulnerability_class: "sql_injection".into(),
            service_type: "http".into(),
            technology_stack: "php,mysql".into(),
            total_attempts: 5,
            successes: 3,
            avg_tool_calls: 4.0,
            avg_duration_secs: 10.0,
            brute_force_needed: false,
            attack_chain: r#"["recon","probe","exploit"]"#.into(),
            first_seen_at: "2026-03-01T00:00:00Z".into(),
            last_seen_at: "2026-03-09T00:00:00Z".into(),
            last_session_id: "sess-1".into(),
        })
        .unwrap();

        db.upsert_attack_pattern(&AttackPattern {
            id: 0,
            technique: "ftp_anon".into(),
            vulnerability_class: "misconfiguration".into(),
            service_type: "ftp".into(),
            technology_stack: "vsftpd".into(),
            total_attempts: 2,
            successes: 2,
            avg_tool_calls: 2.0,
            avg_duration_secs: 3.0,
            brute_force_needed: false,
            attack_chain: r#"["recon","exploit"]"#.into(),
            first_seen_at: "2026-03-01T00:00:00Z".into(),
            last_seen_at: "2026-03-05T00:00:00Z".into(),
            last_session_id: "sess-2".into(),
        })
        .unwrap();

        // Seed session fingerprints
        db.save_fingerprint(&SessionFingerprint {
            session_id: "sess-1".into(),
            services_seen: "http,ssh".into(),
            technologies_seen: "nginx,php".into(),
            vuln_classes_found: "sqli".into(),
            flags_captured: 2,
            hosts_count: 1,
            summary: "Web pentest".into(),
            outcome: "achieved".into(),
            goal_type: "capture-flags".into(),
        })
        .unwrap();

        db.save_fingerprint(&SessionFingerprint {
            session_id: "sess-2".into(),
            services_seen: "ftp,ssh".into(),
            technologies_seen: "vsftpd,openssh".into(),
            vuln_classes_found: "misconfiguration".into(),
            flags_captured: 1,
            hosts_count: 1,
            summary: "FTP pentest".into(),
            outcome: "achieved".into(),
            goal_type: "capture-flags".into(),
        })
        .unwrap();

        db.save_fingerprint(&SessionFingerprint {
            session_id: "sess-3".into(),
            services_seen: "dns".into(),
            technologies_seen: "bind".into(),
            vuln_classes_found: "".into(),
            flags_captured: 0,
            hosts_count: 1,
            summary: "DNS recon".into(),
            outcome: "failed".into(),
            goal_type: "vuln-assessment".into(),
        })
        .unwrap();

        // Seed technique executions
        db.record_execution(&TechniqueExecution {
            id: 0,
            session_id: "sess-1".into(),
            task_type: "differential_probe".into(),
            target_host: "10.0.0.1".into(),
            target_service: "http".into(),
            tool_calls: 3,
            wall_clock_secs: 5.0,
            succeeded: true,
            brute_force_used: false,
            technology_stack: "nginx".into(),
            executed_at: "2026-03-09T10:00:00Z".into(),
        })
        .unwrap();

        db.record_execution(&TechniqueExecution {
            id: 0,
            session_id: "sess-1".into(),
            task_type: "differential_probe".into(),
            target_host: "10.0.0.1".into(),
            target_service: "http".into(),
            tool_calls: 4,
            wall_clock_secs: 7.0,
            succeeded: false,
            brute_force_used: false,
            technology_stack: "nginx".into(),
            executed_at: "2026-03-09T10:05:00Z".into(),
        })
        .unwrap();

        db.record_execution(&TechniqueExecution {
            id: 0,
            session_id: "sess-2".into(),
            task_type: "stack_fingerprint".into(),
            target_host: "10.0.0.2".into(),
            target_service: "ftp".into(),
            tool_calls: 2,
            wall_clock_secs: 3.0,
            succeeded: true,
            brute_force_used: false,
            technology_stack: "vsftpd".into(),
            executed_at: "2026-03-09T10:10:00Z".into(),
        })
        .unwrap();

        // Query for http + nginx
        let query = RelevanceQuery {
            services: vec!["http".into()],
            technologies: vec!["nginx".into()],
            goal_type: Some("capture-flags".into()),
            tags: vec![],
        };
        let intel = db.gather_cross_session_intel(&query).unwrap();

        // Should find the sqli_union pattern (service_type=http)
        assert!(!intel.relevant_patterns.is_empty());
        assert!(
            intel
                .relevant_patterns
                .iter()
                .any(|p| p.technique == "sqli_union")
        );
        // Should NOT find ftp_anon pattern
        assert!(
            !intel
                .relevant_patterns
                .iter()
                .any(|p| p.technique == "ftp_anon")
        );

        // Similar sessions: sess-1 matches (http + nginx + capture-flags = 3),
        // sess-2 does not match (no http or nginx overlap)
        assert!(!intel.similar_sessions.is_empty());
        assert!(
            intel
                .similar_sessions
                .iter()
                .any(|s| s.session_id == "sess-1")
        );
        // sess-3 has no overlap at all
        assert!(
            !intel
                .similar_sessions
                .iter()
                .any(|s| s.session_id == "sess-3")
        );

        // Technique stats: should have differential_probe stats (target_service=http)
        assert!(!intel.technique_stats.is_empty());
        let probe_stats = intel
            .technique_stats
            .iter()
            .find(|s| s.task_type == "differential_probe");
        assert!(probe_stats.is_some());
        let ps = probe_stats.unwrap();
        assert!((ps.avg_tool_calls - 3.5).abs() < 0.01); // (3+4)/2
        assert!((ps.success_rate - 0.5).abs() < 0.01); // 1 of 2 succeeded
        assert!((ps.avg_duration - 6.0).abs() < 0.01); // (5+7)/2

        // Query with empty services/technologies returns empty results
        let empty_query = RelevanceQuery {
            services: vec![],
            technologies: vec![],
            goal_type: None,
            tags: vec![],
        };
        let empty_intel = db.gather_cross_session_intel(&empty_query).unwrap();
        assert!(empty_intel.relevant_patterns.is_empty());
        assert!(empty_intel.similar_sessions.is_empty());
        assert!(empty_intel.technique_stats.is_empty());
    }

    #[test]
    fn test_find_similar_sessions_ranking() {
        let db = Db::open_in_memory().unwrap();

        db.save_fingerprint(&SessionFingerprint {
            session_id: "low-match".into(),
            services_seen: "http".into(),
            technologies_seen: "apache".into(),
            vuln_classes_found: "".into(),
            flags_captured: 0,
            hosts_count: 1,
            summary: "".into(),
            outcome: "".into(),
            goal_type: "capture-flags".into(),
        })
        .unwrap();

        db.save_fingerprint(&SessionFingerprint {
            session_id: "high-match".into(),
            services_seen: "http,ssh".into(),
            technologies_seen: "nginx,php".into(),
            vuln_classes_found: "".into(),
            flags_captured: 0,
            hosts_count: 1,
            summary: "".into(),
            outcome: "".into(),
            goal_type: "capture-flags".into(),
        })
        .unwrap();

        let query = RelevanceQuery {
            services: vec!["http".into(), "ssh".into()],
            technologies: vec!["nginx".into(), "php".into()],
            goal_type: Some("capture-flags".into()),
            tags: vec![],
        };

        let results = db.find_similar_sessions(&query, 10).unwrap();
        assert_eq!(results.len(), 2);
        // high-match should be first (4 overlaps + goal = 5), low-match second (1 overlap + goal = 2)
        assert_eq!(results[0].session_id, "high-match");
        assert_eq!(results[1].session_id, "low-match");
    }

    #[test]
    fn test_session_status_roundtrip() {
        let db = Db::open_in_memory().unwrap();

        for status in [
            SessionStatus::Running,
            SessionStatus::Completed,
            SessionStatus::Interrupted,
        ] {
            let mut session = make_test_session(&format!("status-{status:?}"));
            session.status = status.clone();
            db.save_session(&session).unwrap();
            let loaded = db.load_session(&session.id).unwrap();
            assert_eq!(loaded.status, status);
        }
    }
}
