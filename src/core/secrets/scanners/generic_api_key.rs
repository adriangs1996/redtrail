use regex::Regex;

use crate::core::secrets::engine::{SecretMatch, SecretScanner};

/// Matches long, high-entropy strings near sensitive keywords like
/// api_key, api_token, auth_token, api_secret — in colon-separated
/// or header contexts that the assignment scanner doesn't cover.
struct GenericApiKeyScanner;

fn is_placeholder(val: &str) -> bool {
    let upper = val.to_uppercase();
    upper.contains("YOUR_")
        || upper.contains("_HERE")
        || upper.contains("EXAMPLE")
        || upper.contains("CHANGEME")
        || upper.contains("REPLACE")
}

impl SecretScanner for GenericApiKeyScanner {
    fn scan(&self, input: &str) -> Vec<SecretMatch> {
        let mut matches = Vec::new();

        // Colon-separated: keyword: <long_value> (YAML, config)
        let colon_re = Regex::new(
            r"(?i)(?:api[_-]?key|api[_-]?token|auth[_-]?token|api[_-]?secret|access[_-]?token)\s*:\s*(\S{32,})",
        ).unwrap();
        for cap in colon_re.captures_iter(input) {
            let val = cap.get(1).unwrap();
            if !is_placeholder(val.as_str()) {
                matches.push(SecretMatch {
                    start: val.start(),
                    end: val.end(),
                    label: "api_key".into(),
                });
            }
        }

        // Authorization: Bearer <token> (32+ chars)
        let bearer_re = Regex::new(r"(?i)Authorization:\s*Bearer\s+(\S{32,})").unwrap();
        for cap in bearer_re.captures_iter(input) {
            let val = cap.get(1).unwrap();
            matches.push(SecretMatch {
                start: val.start(),
                end: val.end(),
                label: "api_key".into(),
            });
        }

        // Authorization: Basic <base64> (20+ chars)
        let basic_re = Regex::new(r"(?i)Authorization:\s*Basic\s+([A-Za-z0-9+/=]{20,})").unwrap();
        for cap in basic_re.captures_iter(input) {
            let val = cap.get(1).unwrap();
            matches.push(SecretMatch {
                start: val.start(),
                end: val.end(),
                label: "api_key".into(),
            });
        }

        matches
    }

    fn priority(&self) -> u8 {
        15
    }
}
inventory::submit!(&GenericApiKeyScanner as &dyn SecretScanner);

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(input: &str) -> Vec<SecretMatch> {
        GenericApiKeyScanner.scan(input)
    }

    fn assert_detects(input: &str, msg: &str) {
        let matches = scan(input);
        assert!(!matches.is_empty(), "should detect: {msg} — input: {input}");
    }

    fn assert_ignores(input: &str, msg: &str) {
        let matches = scan(input);
        assert!(
            matches.is_empty(),
            "should NOT detect: {msg} — input: {input}"
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 1. COLON/SPACE SEPARATED KEY-VALUE (YAML, config files)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_api_key_colon_separated() {
        assert_detects(
            "api_key: sk-1234567890abcdef1234567890abcdef",
            "api_key with colon separator and 32+ char value",
        );
    }

    #[test]
    fn detects_api_token_colon_separated() {
        assert_detects(
            "api_token: tok-AAAAAAAAAAAA-BBBBBBBBBBBB-ccccccccddddddddeeeeeeee",
            "api_token with long value",
        );
    }

    #[test]
    fn detects_auth_token_colon_separated() {
        assert_detects(
            "auth_token: abcdef1234567890abcdef1234567890ab",
            "auth_token with 32+ char value",
        );
    }

    #[test]
    fn detects_api_secret_colon_separated() {
        assert_detects(
            "api_secret: abcdef1234567890abcdef1234567890ab",
            "api_secret colon-separated",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 2. AUTHORIZATION HEADERS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_bearer_token_in_curl() {
        assert_detects(
            "curl -H 'Authorization: Bearer sk-abcdef1234567890abcdef1234567890' https://api.example.com",
            "Bearer token in curl header",
        );
    }

    #[test]
    fn detects_basic_auth_in_header() {
        assert_detects(
            "Authorization: Basic dXNlcjpwYXNzd29yZDEyMzQ1Njc4OTA=",
            "Basic auth base64 credentials",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 3. MATCH METADATA
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn match_covers_only_value() {
        let input = "api_key: sk-1234567890abcdef1234567890abcdef";
        let matches = scan(input);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            &input[matches[0].start..matches[0].end],
            "sk-1234567890abcdef1234567890abcdef"
        );
    }

    #[test]
    fn label_is_api_key() {
        let input = "api_key: sk-1234567890abcdef1234567890abcdef";
        let matches = scan(input);
        assert_eq!(matches[0].label, "api_key");
    }

    // ──────────────────────────────────────────────────────────────
    // 4. TRUE NEGATIVES
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn ignores_short_value_near_keyword() {
        assert_ignores("api_key: abc", "short value — not a real key");
    }

    #[test]
    fn ignores_keyword_without_value() {
        assert_ignores("api_key", "keyword alone, no value");
    }

    #[test]
    fn ignores_normal_text() {
        assert_ignores("the quick brown fox", "no API key context");
    }

    #[test]
    fn ignores_empty_input() {
        assert_ignores("", "empty input");
    }

    #[test]
    fn ignores_placeholder_values() {
        assert_ignores(
            "api_key: YOUR_API_KEY_HERE",
            "placeholder value — not a real secret",
        );
    }
}
