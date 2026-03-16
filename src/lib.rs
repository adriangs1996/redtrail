pub mod agent;
pub mod completion;
pub mod backend;
pub mod db;
pub mod db_v2;
pub mod error;
pub mod middleware;
pub mod report;
pub mod tui;
pub mod types;
pub mod workflows;

pub use agent::attack_graph::{AttackEdge, AttackGraph, AttackNode, CredentialType};
pub use agent::claude_executor::{
    ClaudeExecutor, CredentialResult, FindingReport, ParseOutcome, SpecialistResult,
};
pub use agent::flags::FlagDetector;
pub use agent::llm::{
    AnthropicApiConfig, LlmConfig, LlmError, LlmProvider, OllamaConfig, create_provider,
};
pub use agent::query_agent;
pub use agent::strategist::{AdvisorSuggestion, Suggestion};
pub use agent::{AgentMessage, AgentState, Role, ToolCall};
pub use backend::Backend;
pub use backend::knowledge::KnowledgeBase;
pub use backend::knowledge::types::{
    AccessLevel, AssessmentDepth, AttackPath, BypassTechnique, Criterion, CriterionCheck,
    DeductiveMetrics, DefenderModel, DetectedWaf, DetectionCost, EvidenceChain, EvidenceRecord,
    FailedAttempt, GoalStatus, GoalType, HostInfo, HypothesisCategory, HypothesisStatus,
    IdsSensitivity, PocScript, RateLimit, ScanSession, SessionGoal, SessionStatus, SessionSummary,
    TaskSummary, WafType,
};
pub use db::Db;
pub use error::Error;
pub use report::generator::{
    HistoricalMetrics, ReportError, calculate_score, fix_suggestion_for, generate_html,
    generate_html_with_flags, generate_html_with_history, generate_html_with_knowledge,
    generate_report, generate_report_with_knowledge, score_color,
};
pub use types::{
    AttackSurface, Credential, Endpoint, Evidence, ExecMode, Finding, HttpRequest, HttpResponse,
    Parameter, ParameterLocation, Severity, Target, VulnType,
};
pub use workflows::{ShellCommand, CommandOutput, Block, BlockStatus, ShellOutputLine, ShellOutputStream, BlockContent, TableData};
