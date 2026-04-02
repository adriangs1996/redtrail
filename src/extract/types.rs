use crate::core::db::CommandRow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Domain {
    Git,
    Docker,
    Generic,
}

#[derive(Debug, Clone)]
pub struct Extraction {
    pub entities: Vec<NewEntity>,
    pub relationships: Vec<NewRelationship>,
}

impl Extraction {
    pub fn empty() -> Self {
        Self {
            entities: Vec::new(),
            relationships: Vec::new(),
        }
    }

    pub fn merge(&mut self, other: Extraction) {
        self.entities.extend(other.entities);
        self.relationships.extend(other.relationships);
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty() && self.relationships.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct NewEntity {
    pub entity_type: String,
    pub name: String,
    pub canonical_key: String,
    pub properties: Option<serde_json::Value>,
    pub typed_data: Option<TypedEntityData>,
    pub observation_context: Option<String>,
}

#[derive(Debug, Clone)]
pub enum TypedEntityData {
    GitBranch {
        repo: String,
        name: String,
        is_remote: bool,
        remote_name: Option<String>,
        upstream: Option<String>,
        ahead: Option<i32>,
        behind: Option<i32>,
        last_commit_hash: Option<String>,
    },
    GitCommit {
        repo: String,
        hash: String,
        short_hash: Option<String>,
        author_name: Option<String>,
        author_email: Option<String>,
        message: Option<String>,
        committed_at: Option<i64>,
    },
    GitRemote {
        repo: String,
        name: String,
        url: Option<String>,
    },
    GitFile {
        repo: String,
        path: String,
        status: Option<String>,
        insertions: Option<i32>,
        deletions: Option<i32>,
    },
    GitTag {
        repo: String,
        name: String,
        commit_hash: Option<String>,
    },
    GitStash {
        repo: String,
        index_num: i32,
        message: String,
    },
    DockerContainer {
        container_id: Option<String>,
        name: String,
        image: Option<String>,
        status: Option<String>,
        ports: Option<String>,
    },
    DockerImage {
        repository: String,
        tag: Option<String>,
        image_id: Option<String>,
        size_bytes: Option<i64>,
    },
    DockerNetwork {
        name: String,
        network_id: Option<String>,
        driver: Option<String>,
    },
    DockerVolume {
        name: String,
        driver: Option<String>,
        mountpoint: Option<String>,
    },
    DockerService {
        name: String,
        image: Option<String>,
        compose_file: Option<String>,
        ports: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct NewRelationship {
    pub source_canonical_key: String,
    pub source_type: String,
    pub target_canonical_key: String,
    pub target_type: String,
    pub relation_type: String,
    pub properties: Option<serde_json::Value>,
}

#[derive(Debug)]
pub enum ExtractError {
    Parse(String),
    Db(String),
    NoOutput,
    UnsupportedCommand,
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(msg) => write!(f, "parse error: {msg}"),
            Self::Db(msg) => write!(f, "db error: {msg}"),
            Self::NoOutput => write!(f, "no output to extract from"),
            Self::UnsupportedCommand => write!(f, "unsupported command"),
        }
    }
}

pub trait DomainExtractor {
    fn domain(&self) -> Domain;
    fn can_handle(&self, binary: &str, subcommand: Option<&str>) -> bool;
    fn extract(&self, cmd: &CommandRow) -> Result<Extraction, ExtractError>;
}
