# Phase 2: Intelligent Extraction — Design Spec

> Approved: 2026-04-01
> Status: Ready for implementation planning

---

## Goal

Turn raw command captures into structured entities and relationships. The extractor pipeline is the core technical differentiator — it transforms a stream of terminal commands into a queryable knowledge graph.

## Prerequisites

### Streaming Capture for Long-Running Commands (separate task)

The current capture layer only writes to the DB when a command finishes (`precmd`). Long-running commands like web servers (`rails s`, `npm run dev`) produce valuable output (port bindings, errors, logs) that sits invisible until the process is killed.

**Required change:** Split capture into `--start` (at preexec, inserts a `status='running'` record) and `--finish` (at precmd, finalizes with exit code). The tee process periodically flushes stdout to the DB row while the command runs. This enables real-time extraction from server output — e.g., detecting port 3000 seconds after `rails s` starts, not hours later when it's killed.

This is a Phase 1 capture layer change planned as its own task. Phase 2 extraction works without it (processes finished commands only) but gains significant value from it (live entity extraction from running commands).

---

## Design Decisions

### 1. Language: All Rust

The entire extractor pipeline is implemented in Rust within the existing binary. No Python introduction.

**Rationale:** Single binary deployment, no polyglot build complexity, heuristic parsers are fast and don't need Python's flexibility.

### 1b. LLM Fallback (Late Phase 2)

The GUIDELINE requires "LLM fallback works with Ollama locally" as a Phase 2 acceptance criterion. This is implemented in the final stretch of Phase 2 after heuristic extractors are solid. The LLM extractor is another implementation of the `DomainExtractor` trait that sends unstructured stdout/stderr to a local Ollama instance and parses the structured response. It runs only in Pass 2 (`redtrail extract --llm`) — never in the inline capture path.

The Rust implementation uses HTTP calls to Ollama's local API (`http://localhost:11434/api/generate`). No Python needed. If Ollama is not running or not installed, the LLM path is skipped gracefully — extraction_method stays `partial`.

### 2. Build Order

1. Git extractors (highest frequency for both humans and agents)
2. Docker extractors (second domain, required by GUIDELINE acceptance criteria)
3. Generic extractor (fallback — file paths, IPs, URLs, ports, usernames, errors)

**Note:** The GUIDELINE specifies Git -> Docker -> Generic. We follow that order. The generic extractor is simpler but comes last because it's supplementary — it always runs alongside domain extractors as a fallback, but its standalone implementation is lower priority than getting the two primary domains right.

### 2b. Future Domains (Out of Scope for Phase 2)

The `DomainExtractor` trait is designed to accommodate additional domains in later phases. The GUIDELINE's domain detector table includes: kubernetes, infrastructure, package_management, network, database, testing, and systemd. These will be added as the product matures — the trait-based architecture means each is an isolated implementation with no impact on existing extractors.

### 3. Extraction Trigger: Hybrid (Option C+)

- **Heuristic extraction runs inline** after capture for known domains (git, docker). Adds ~5-10ms to the post-prompt `precmd` path — the user's prompt is already displayed.
- **`redtrail extract`** CLI for explicit batch reprocessing of historical data.
- **Lazy extraction on query** — `redtrail context` and `redtrail entities` process any remaining unextracted commands before rendering.
- **Two-pass model** for LLM:
  - Pass 1 (immediate, no LLM): domain extractor succeeds, or falls back to generic extractor (partial) + queues for LLM.
  - Pass 2 (deferred): `redtrail extract --llm` processes the LLM queue. The daemon takes this over in Phase 4.

### 4. `redtrail context` v1: Pure Data Report

Structured sections with headers, no LLM dependency. Displays factual data from extracted entities. LLM narrative summary is a future enhancement.

---

## Data Model

### Philosophy: Class Inheritance via SQL

Entity types are modeled as a class hierarchy. The `entities` table is the base class (`Object`). Domain-specific tables are subclasses with typed columns, linked back to `entities` via foreign key.

Generic/unknown entities (file paths, IPs, URLs) live only in the `entities` base table with a JSON `properties` blob. Domain entities get a row in both `entities` and their typed table.

### Base Table (Object)

```sql
-- Base class: every entity gets a row here
CREATE TABLE entities (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,        -- 'git_branch', 'git_commit', 'docker_container', 'file', 'url', ...
    name TEXT NOT NULL,        -- human-readable label
    canonical_key TEXT NOT NULL, -- dedup key: type-specific unique identifier
    properties TEXT,           -- JSON blob (primary store for generic types, optional for typed)
    first_seen INTEGER NOT NULL,
    last_seen INTEGER NOT NULL,
    source_command_id TEXT REFERENCES commands(id),
    UNIQUE(type, canonical_key)  -- enforces dedup across ALL entity types
);

CREATE INDEX idx_entities_type_name ON entities(type, name);
CREATE INDEX idx_entities_last_seen ON entities(last_seen);
```

The `canonical_key` column is the universal dedup mechanism. For domain entities it mirrors the typed table's UNIQUE constraint (e.g., `{repo}:{name}:{is_remote}` for branches). For generic entities (file, url, ip_address) that have no typed table, it's the only dedup mechanism — without it, repeated extractions of the same file path would create duplicate entities.

### Entity Observations (Junction)

Every time a command references an entity, log the observation. This enables "when did this branch first appear", "which commands mentioned it", and richer pattern mining in Phase 4.

```sql
CREATE TABLE entity_observations (
    id TEXT PRIMARY KEY,
    entity_id TEXT NOT NULL REFERENCES entities(id),
    command_id TEXT NOT NULL REFERENCES commands(id),
    observed_at INTEGER NOT NULL,
    context TEXT,              -- optional: what role the entity played ('created', 'modified', 'read', 'deleted')
    UNIQUE(entity_id, command_id)
);

CREATE INDEX idx_entity_obs_entity ON entity_observations(entity_id, observed_at);
CREATE INDEX idx_entity_obs_command ON entity_observations(command_id);
```

### Git Domain Tables

```sql
CREATE TABLE git_branches (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    repo TEXT NOT NULL,
    name TEXT NOT NULL,
    is_remote BOOLEAN DEFAULT 0,
    remote_name TEXT,
    upstream TEXT,
    ahead INTEGER,
    behind INTEGER,
    last_commit_hash TEXT,
    UNIQUE(repo, name, is_remote)
);

CREATE TABLE git_commits (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    repo TEXT NOT NULL,
    hash TEXT NOT NULL,
    short_hash TEXT,
    author_name TEXT,
    author_email TEXT,
    message TEXT,
    committed_at INTEGER,
    UNIQUE(repo, hash)
);

CREATE TABLE git_remotes (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    repo TEXT NOT NULL,
    name TEXT NOT NULL,
    url TEXT,
    UNIQUE(repo, name)
);

CREATE TABLE git_files (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    repo TEXT NOT NULL,
    path TEXT NOT NULL,           -- relative to repo root
    status TEXT,                  -- 'modified', 'staged', 'untracked', 'deleted', 'renamed'
    insertions INTEGER,
    deletions INTEGER,
    UNIQUE(repo, path)
);

CREATE TABLE git_tags (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    repo TEXT NOT NULL,
    name TEXT NOT NULL,
    commit_hash TEXT,
    UNIQUE(repo, name)
);

CREATE TABLE git_stashes (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    repo TEXT NOT NULL,
    index_num INTEGER,           -- current index (shifts on push/pop, informational only)
    message TEXT NOT NULL,
    UNIQUE(repo, message)        -- message is stable; index_num shifts
);
```

### Docker Domain Tables

```sql
CREATE TABLE docker_containers (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    container_id TEXT,
    name TEXT NOT NULL,
    image TEXT,
    status TEXT,              -- 'running', 'exited', 'created', 'paused'
    ports TEXT,               -- JSON: [{"host": 8080, "container": 80, "protocol": "tcp"}]
    created_at_container INTEGER,
    UNIQUE(name)
);

CREATE TABLE docker_images (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    repository TEXT NOT NULL,
    tag TEXT,
    image_id TEXT,
    size_bytes INTEGER,
    created_at_image INTEGER,
    UNIQUE(repository, tag)
);

CREATE TABLE docker_networks (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    name TEXT NOT NULL,
    network_id TEXT,
    driver TEXT,
    UNIQUE(name)
);

CREATE TABLE docker_volumes (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    name TEXT NOT NULL,
    driver TEXT,
    mountpoint TEXT,
    UNIQUE(name)
);

CREATE TABLE docker_services (
    entity_id TEXT PRIMARY KEY REFERENCES entities(id),
    name TEXT NOT NULL,
    image TEXT,
    compose_file TEXT,           -- path to docker-compose.yml
    ports TEXT,                  -- JSON array
    UNIQUE(name, compose_file)
);
```

### Relationships (Polymorphic)

```sql
CREATE TABLE relationships (
    id TEXT PRIMARY KEY,
    source_entity_id TEXT NOT NULL REFERENCES entities(id),
    target_entity_id TEXT NOT NULL REFERENCES entities(id),
    type TEXT NOT NULL,        -- 'modified', 'belongs_to', 'points_to', 'authored_by', 'deployed_to'
    properties TEXT,           -- optional JSON
    observed_at INTEGER,
    source_command_id TEXT REFERENCES commands(id)
);

CREATE INDEX idx_relationships_source ON relationships(source_entity_id);
CREATE INDEX idx_relationships_target ON relationships(target_entity_id);
CREATE INDEX idx_relationships_type ON relationships(type);
```

### Extraction Metadata on Commands

The existing `extracted` boolean is extended with an extraction method column:

```sql
ALTER TABLE commands ADD COLUMN extraction_method TEXT;
-- Values: NULL (not extracted), 'heuristic', 'generic', 'partial', 'llm', 'skipped'
```

- `heuristic`: domain extractor succeeded fully
- `generic`: only the generic fallback ran
- `partial`: generic ran, queued for LLM enrichment
- `llm`: LLM enrichment completed
- `skipped`: nothing extractable (e.g., `cd`, `ls` with no output)

---

## Extractor Pipeline

### Command Parsing: Pipes, Chains, and Redirects

The `command_raw` field may contain pipes (`|`), chains (`&&`, `||`, `;`), redirects (`>`, `2>&1`), and subshells (`$(...)`). The command parser splits these into segments before extraction:

1. **Split on pipes and chains** — `git status | grep modified` becomes two segments: `git status` and `grep modified`.
2. **Each segment is extracted independently** — the git extractor handles the first, generic handles the second.
3. **Redirects are stripped** — `cargo build 2>&1 > /dev/null` becomes `cargo build`.
4. **Subshells are not expanded** — `echo $(git branch)` extracts from `echo` (generic). We don't have the subshell's stdout separately.
5. **The `shell-words` crate handles quoting** within each segment, but segment splitting (pipes/chains) happens first via a simple state-machine tokenizer that respects quotes and escapes.

The stdout/stderr captured in the DB corresponds to the **entire pipeline's** output, not individual segments. So the domain extractor for the *last meaningful command* in a pipeline gets the stdout. For `git log | head -20`, the git extractor receives the truncated output and must handle it gracefully (partial parse is fine).

### Architecture

```
for each unextracted command:
  1. split command_raw into segments (pipes, chains)
  2. for each segment: parse -> (binary, subcommand, flags, args)
  3. detect domain from binary of primary segment (first, or most specific)
  4. run domain extractor (if available) — receives full stdout/stderr
     - success -> entities + relationships
     - failure -> fall through
  4. run generic extractor (always, as fallback or supplement)
  5. entity resolution: for each entity, upsert via canonical key
  6. insert entity_observations
  7. insert relationships
  8. mark command: extracted = 1, extraction_method = <method>
  9. if domain extractor failed and output is non-trivial -> queue for LLM (extraction_method = 'partial')
```

### Extractor Trait

```rust
pub struct Extraction {
    pub entities: Vec<NewEntity>,
    pub relationships: Vec<NewRelationship>,
}

pub struct NewEntity {
    pub entity_type: String,       // "git_branch", "git_commit", "file", "url"
    pub name: String,              // human-readable label
    pub canonical_key: String,     // for dedup/resolution
    pub properties: Option<String>, // JSON for generic types
    pub typed_data: Option<TypedEntityData>, // enum for domain-specific columns
    pub observation_context: Option<String>, // 'created', 'modified', 'read'
}

pub enum TypedEntityData {
    GitBranch { repo: String, name: String, is_remote: bool, remote_name: Option<String>, upstream: Option<String>, ahead: Option<i32>, behind: Option<i32>, last_commit_hash: Option<String> },
    GitCommit { repo: String, hash: String, short_hash: Option<String>, author_name: Option<String>, author_email: Option<String>, message: Option<String>, committed_at: Option<i64> },
    GitRemote { repo: String, name: String, url: Option<String> },
    GitFile { repo: String, path: String, status: Option<String>, insertions: Option<i32>, deletions: Option<i32> },
    GitTag { repo: String, name: String, commit_hash: Option<String> },
    GitStash { repo: String, index_num: i32, message: Option<String> },
    DockerContainer { container_id: Option<String>, name: String, image: Option<String>, status: Option<String>, ports: Option<String> },
    DockerImage { repository: String, tag: Option<String>, image_id: Option<String>, size_bytes: Option<i64> },
    DockerNetwork { name: String, network_id: Option<String>, driver: Option<String> },
    DockerVolume { name: String, driver: Option<String>, mountpoint: Option<String> },
    DockerService { name: String, image: Option<String>, compose_file: Option<String>, ports: Option<String> },
}

pub struct NewRelationship {
    pub source_canonical_key: String,
    pub target_canonical_key: String,
    pub relation_type: String,
    pub properties: Option<String>,
}

pub trait DomainExtractor {
    fn domain(&self) -> &str;
    fn can_handle(&self, binary: &str, subcommand: Option<&str>) -> bool;
    fn extract(&self, cmd: &CommandRow) -> Result<Extraction, ExtractError>;
}
```

### Entity Resolution

On each `NewEntity`:

1. Compute canonical key from entity type + type-specific fields (e.g., `git_branch:{repo}:{name}:{is_remote}`)
2. Query `entities` for existing row with same `type` and matching canonical key (via typed table UNIQUE constraint)
3. **If exists:** update `last_seen`, merge properties, update typed columns if new data is richer, insert `entity_observations` row
4. **If new:** insert into `entities`, insert into typed table (if domain type), insert `entity_observations` row

Canonical key definitions:

| Entity Type | Canonical Key |
|---|---|
| git_branch | `{repo}:{name}:{is_remote}` |
| git_commit | `{repo}:{hash}` |
| git_remote | `{repo}:{name}` |
| git_file | `{repo}:{path}` |
| git_tag | `{repo}:{name}` |
| git_stash | `{repo}:{message_hash}` (hash of stash message, since index_num shifts on push/pop) |
| file | absolute path |
| url | full URL |
| ip_address | IP string |
| port | `{number}:{protocol}` |
| process | `{binary}:{pid}` |

### Git Extractor: Subcommand Handlers

The git extractor dispatches by subcommand:

| Subcommand | Parser Strategy | Entities Produced | Relationships Produced |
|---|---|---|---|
| `git status` | Parse human-readable output: "modified:", "new file:", "deleted:", "Untracked files:" sections | git_file (with status) | file `belongs_to` repo |
| `git log` | Parse commit blocks: hash, Author, Date, message. Handle `--oneline` format. | git_commit | commit `authored_by` person (generic entity), commit `belongs_to` repo |
| `git diff` | Parse `diff --git` headers + `@@ ... @@` hunks. Parse `--stat` format. | git_file (with insertions/deletions) | file `belongs_to` repo |
| `git branch` | Parse branch listing. Detect `*` for current. Parse `-a` remote branches. | git_branch | branch `belongs_to` repo |
| `git remote -v` | Parse `name\turl (fetch/push)` lines | git_remote | remote `belongs_to` repo |
| `git stash list` | Parse `stash@{N}: ...` lines | git_stash | stash `belongs_to` repo |
| `git tag` | One tag per line | git_tag | tag `belongs_to` repo, tag `points_to` commit (if hash available) |

**Note on repo entities:** The repo itself is a generic entity (type `git_repo`, canonical_key = repo root path). It's created implicitly on the first extraction from a given `commands.git_repo` value. All `belongs_to` relationships point to it.

**Defensive parsing rules:**
- Strip ANSI color codes before parsing
- If output is empty or unparseable, return empty Extraction (never error)
- Handle common aliases: `git st` -> status, `git co` -> checkout
- If stdout is compressed in the DB, decompress transparently before extraction

### Generic Extractor

Runs on every command as fallback. Uses regex patterns:

| Pattern | Entity Type | Relationships |
|---|---|---|
| Absolute/relative file paths | file | — |
| `\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}` | ip_address | — |
| `https?://...` | url | — |
| `:\d{2,5}` in context (listen, bind, port) | port | port `bound_by` process (if co-extracted) |
| PID + binary from `lsof`, `ss -tlnp`, `ps`, `netstat` output | process | process `listens_on` port |
| `user@`, `User:`, SSH-style `user@host` | username | — |
| Common error patterns (with exit_code != 0) | error_signature (in properties) | — |

**Process-port correlation:** Port-process links come from two sources:

1. **Diagnostic tools:** `lsof -i :3000`, `ss -tlnp`, `netstat -tlnp` — output explicitly shows PID + binary + port together.
2. **Server stdout:** Any command whose output announces a listening port. `rails s` prints `Listening on http://127.0.0.1:3000`, `npm run dev` prints `Local: http://localhost:5173`, `python manage.py runserver` prints `Starting development server at http://127.0.0.1:8000/`. The generic extractor matches patterns like "listening on", "serving on/at", "started at/on", "http://...:\d+" in stdout and creates a port entity linked to the command's binary as the process.

In both cases, the extractor creates both entities and links them via `listens_on` / `bound_by` relationships. The process entity uses canonical key `{binary}:{pid}` when PID is available, or `{binary}:{cwd}` for server commands where PID isn't in stdout (the cwd scopes it to the project). The `entity_observations` table tracks when each was active, enabling temporal queries.

This means RedTrail passively learns the development port map for every project: which ports are used, by which tools, in which directory — without the developer ever running a diagnostic command.

**Truncated output handling:** When `stdout_truncated` or `stderr_truncated` flags are set on a command, the extractor still runs on whatever output is available. If the domain parser hits an incomplete parse (e.g., truncated mid-line), it returns what it could extract and the command is marked `extraction_method = 'partial'` rather than `'heuristic'`.

---

## CLI Commands

### `redtrail extract`

```bash
redtrail extract                       # process all unextracted commands
redtrail extract --reprocess           # re-extract all commands (reset extracted flag)
redtrail extract --since "7d"          # only commands from last 7 days
redtrail extract --llm                 # run LLM pass on queued commands (Phase 2b)
redtrail extract --dry-run             # show what would be extracted without writing
```

### `redtrail entities`

```bash
redtrail entities                          # list all known entities
redtrail entities --type git_branch        # filter by type
redtrail entities --type git_file --repo . # filter by type + current repo
redtrail entities --json                   # JSON output
```

### `redtrail context`

```bash
redtrail context                   # project context for current directory
redtrail context --format markdown # markdown output (for AI agents)
redtrail context --format json     # JSON output (for AI agents)
```

**Default output sections (git repo):**

```
## Branch
main (2 ahead, 0 behind origin/main)

## Recent Commits (last 5)
acf114a  Rewrite testing for better assertions       (2h ago)
4d7221a  fix: handle non-zero exit in resolve         (5h ago)
13f10b1  fix: use exitCode, add sleeps for ordering   (8h ago)

## Uncommitted Changes
M  src/core/extractor.rs
M  src/cmd/context.rs
?  src/extract/git.rs

## Remotes
origin  git@github.com:user/redtrail.git

## Recent Errors (last 7d)
cargo test -> exit 1 (3 occurrences)
  Last: 1h ago

## Session Activity
Last session: 32 commands over 2h14m
Focus: src/core/ (18 commands), src/cmd/ (8 commands)
Errors: 3 total, 2 resolved in-session
```

### `redtrail entity <id>`

```bash
redtrail entity <id>                   # show entity details
redtrail entity <id> --relationships   # include relationships
redtrail entity <id> --history         # show observation history (which commands referenced it)
```

---

## Module Structure

New modules added to the codebase:

```
src/
  extract/
    mod.rs              # pipeline orchestration, extract trait, entity resolution
    parse.rs            # command string parsing (shell-words)
    domain.rs           # binary -> domain mapping
    git.rs              # git domain extractor
    generic.rs          # generic fallback extractor
    docker.rs           # docker domain extractor (Phase 2b)
    types.rs            # Extraction, NewEntity, NewRelationship, TypedEntityData
  cmd/
    extract.rs          # `redtrail extract` CLI command
    entities.rs         # `redtrail entities` CLI command
    context.rs          # `redtrail context` CLI command
    entity.rs           # `redtrail entity <id>` CLI command
```

The `src/extract/` module is self-contained. It depends on `src/core/db.rs` for DB access and `CommandRow` but has no dependencies on CLI or capture. The capture layer calls into `extract` after writing, but `extract` works independently on any `CommandRow`.

---

## Schema Migration

The new tables and columns must be added via migration in `db.rs`:

1. Add `extraction_method TEXT` column to `commands`
2. Add `canonical_key TEXT` column to `entities` + add UNIQUE index on `(type, canonical_key)`
3. Create `entity_observations` table
4. Create all `git_*` typed tables
5. Create all `docker_*` typed tables (containers, images, networks, volumes, services)
6. Existing `relationships` table already exists — no changes needed

Migration is idempotent (CREATE TABLE IF NOT EXISTS, ALTER TABLE with existence check).

**Note:** This schema is a significant evolution beyond the GUIDELINE's baseline `entities`/`relationships` tables. The typed sub-tables, `canonical_key` column, and `entity_observations` junction table are new designs validated during brainstorming. Update GUIDELINE.md to reflect these additions after implementation lands.

---

## Acceptance Criteria

From the GUIDELINE, with our design decisions applied:

### Extractors
- [ ] Git extractor correctly parses: status, log, diff, branch, remote, tag, stash
- [ ] Docker extractor correctly parses: ps, images, build output
- [ ] Generic extractor finds file paths, IPs, URLs, ports, usernames in arbitrary output
- [ ] All extractors have unit tests with real-world output samples

### Data Model
- [ ] Entity resolution correctly upserts via canonical keys (UNIQUE(type, canonical_key))
- [ ] Entity observations are logged for every extraction
- [ ] Typed tables (git_*, docker_*) populated with correct domain-specific columns
- [ ] Relationships correctly link entities (belongs_to, authored_by, points_to, etc.)

### CLI
- [ ] `redtrail extract` processes unextracted commands in batch
- [ ] `redtrail entities` shows extracted entities, filterable by type and repo
- [ ] `redtrail context` produces a useful project summary for a git repo
- [ ] `redtrail context --format json` outputs valid JSON consumable by an LLM
- [ ] `redtrail entity <id> --relationships` shows related entities

### Performance & Robustness
- [ ] Extraction runs inline after capture for heuristic domains (<10ms added)
- [ ] Extraction of heuristic parsers completes in <100ms per command
- [ ] Extraction failures never crash capture or the CLI
- [ ] Commands with failed domain extraction are queued for LLM (extraction_method = 'partial')
- [ ] ANSI codes stripped before parsing
- [ ] Compressed stdout/stderr decompressed transparently before extraction
- [ ] Truncated output handled gracefully (partial extraction, not crash)
- [ ] Piped/chained commands split into segments and extracted independently

### LLM Fallback
- [ ] LLM fallback works with Ollama locally via `redtrail extract --llm`
- [ ] LLM fallback degrades gracefully when Ollama is not available
- [ ] LLM never runs in the inline capture path
