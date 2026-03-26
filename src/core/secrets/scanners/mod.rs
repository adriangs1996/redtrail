mod aws_key;
mod github_token;
mod jwt;
mod sensitive_assignment;
mod sensitive_json_field;

/// Derive a redaction label from a variable/field name.
pub(crate) fn classify_key(key: &str) -> String {
    let lower = key.to_lowercase();
    if lower.contains("password") || lower.contains("passwd") {
        "password".into()
    } else {
        "secret".into()
    }
}
