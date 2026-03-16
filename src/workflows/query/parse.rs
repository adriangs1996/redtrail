use crate::error::Error;

const ALLOWED_PREFIXES: &[&str] = &["select", "pragma", "explain"];

pub fn validate(raw: &str) -> Result<String, Error> {
    let trimmed = raw.trim();
    let lower = trimmed.to_lowercase();

    if !ALLOWED_PREFIXES.iter().any(|p| lower.starts_with(p)) {
        return Err(Error::Validation(
            "read-only: only SELECT, PRAGMA, and EXPLAIN statements are allowed".into()
        ));
    }
    Ok(trimmed.to_string())
}
