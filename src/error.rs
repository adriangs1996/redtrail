use crate::agent::llm::LlmError;
use crate::report::generator::ReportError;

/// Unified error type for the Redtrail scanner.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),

    #[error("report error: {0}")]
    Report(#[from] ReportError),

    #[error("authentication error: {0}")]
    Auth(String),

    #[error("invalid LLM response: {0}")]
    Parse(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database error: {0}")]
    Db(String),

    #[error("validation error: {0}")]
    Validation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probe_error_from_llm_error() {
        let llm_err = LlmError::Timeout(60);
        let probe_err: Error = llm_err.into();
        assert!(probe_err.to_string().contains("timed out after 60s"));
    }

    #[test]
    fn test_probe_error_from_report_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let report_err = ReportError::IoError(io_err);
        let probe_err: Error = report_err.into();
        assert!(probe_err.to_string().contains("file missing"));
    }

    #[test]
    fn test_probe_error_auth_display() {
        let err = Error::Auth("bad credentials".into());
        assert_eq!(err.to_string(), "authentication error: bad credentials");
    }

    #[test]
    fn test_probe_error_parse_display() {
        let err = Error::Parse("not JSON".into());
        assert_eq!(err.to_string(), "invalid LLM response: not JSON");
    }
}
