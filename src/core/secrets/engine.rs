/// A match found by a secret scanner.
pub struct SecretMatch {
    pub start: usize,
    pub end: usize,
    pub label: String,
}

/// Implement this trait and submit it via `inventory::submit!` to register a
/// new secret detection pattern.
///
/// ```ignore
/// struct MyScanner;
/// impl SecretScanner for MyScanner {
///     fn scan(&self, input: &str) -> Vec<SecretMatch> { /* ... */ }
///     fn priority(&self) -> u8 { 0 }
/// }
/// inventory::submit!(&MyScanner as &dyn SecretScanner);
/// ```
pub trait SecretScanner: Send + Sync {
    /// Return all matches found in `input`.
    fn scan(&self, input: &str) -> Vec<SecretMatch>;

    /// Lower = runs first. Value-based scanners should use 0, context-based
    /// scanners should use 10+ so they can skip ranges already covered.
    fn priority(&self) -> u8 {
        0
    }
}

inventory::collect!(&'static dyn SecretScanner);

fn ranges_overlap(s1: usize, e1: usize, s2: usize, e2: usize) -> bool {
    s1 < e2 && s2 < e1
}

fn already_covered(secrets: &[SecretMatch], start: usize, end: usize) -> bool {
    secrets
        .iter()
        .any(|s| ranges_overlap(s.start, s.end, start, end))
}

fn scan_secrets(input: &str) -> Vec<SecretMatch> {
    let mut scanners: Vec<&&dyn SecretScanner> = inventory::iter::<&dyn SecretScanner>().collect();
    scanners.sort_by_key(|s: &&&dyn SecretScanner| s.priority());

    let mut secrets: Vec<SecretMatch> = Vec::new();

    for scanner in scanners {
        for m in (*scanner).scan(input) {
            if !already_covered(&secrets, m.start, m.end) {
                secrets.push(m);
            }
        }
    }

    secrets.sort_by(|a, b| b.start.cmp(&a.start));
    secrets
}

pub fn redact_secrets(input_or_output: &str) -> String {
    redact_secrets_with_labels(input_or_output).0
}

/// Redact secrets and return (redacted_string, list of pattern labels found).
pub fn redact_secrets_with_labels(input_or_output: &str) -> (String, Vec<String>) {
    let secrets = scan_secrets(input_or_output);
    if secrets.is_empty() {
        return (input_or_output.to_string(), Vec::new());
    }
    let labels: Vec<String> = secrets.iter().map(|s| s.label.clone()).collect();
    let mut result = input_or_output.to_string();
    for s in &secrets {
        result.replace_range(s.start..s.end, &format!("[REDACTED:{}]", s.label));
    }
    (result, labels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_secrets_with_key_str() {
        let input = "export AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE";
        let expected = "export AWS_SECRET_ACCESS_KEY=[REDACTED:aws_key]";
        assert_eq!(redact_secrets(input), expected);
    }

    #[test]
    fn redact_secrets_in_requests() {
        let input = "curl -H 'Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U https://api.example.com";
        let expected = "curl -H 'Authorization: Bearer [REDACTED:jwt] https://api.example.com";
        assert_eq!(redact_secrets(input), expected);
    }

    #[test]
    fn redact_secrets_with_password() {
        let input = "echo password=hunter2";
        let expected = "echo password=[REDACTED:password]";
        assert_eq!(redact_secrets(input), expected);
    }

    #[test]
    fn redact_secrets_whit_no_secrets() {
        let input = "ssh-keygen -t rsa -b 4096 -C";
        assert_eq!(redact_secrets(input), input);
    }

    #[test]
    fn redact_secrets_with_variables() {
        let input = "echo $AWS_ACCESS_KEY_ID";
        assert_eq!(redact_secrets(input), input);
    }

    #[test]
    fn redact_secrets_should_redact_github_token() {
        let input = "GITHUB_TOKEN=ghp_1234567890abcdef1234567890abcdef12345678 ./deploy.sh";
        let expected = "GITHUB_TOKEN=[REDACTED:github_token] ./deploy.sh";
        assert_eq!(redact_secrets(input), expected);
    }

    #[test]
    fn redact_secrets_should_redact_secrets_in_output_json() {
        let input = "{\"access_key\": \"AKIAIOSFODNN7EXAMPLE\",\"secret_key\": \"wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\",\"token\": \"FwoGZXIvYXdzEBYaDHqa0...\"}";
        let expected = "{\"access_key\": \"[REDACTED:aws_key]\",\"secret_key\": \"[REDACTED:aws_key]\",\"token\": \"FwoGZXIvYXdzEBYaDHqa0...\"}";
        assert_eq!(redact_secrets(input), expected);
    }
}
