use regex::Regex;

use super::classify_key;
use crate::core::secrets::engine::{SecretMatch, SecretScanner};

/// Strong keywords always trigger regardless of value.
/// Weak keywords (TOKEN, KEY) only trigger when the value is >= 10 chars.
const STRONG_KEYWORDS: &[&str] = &[
    "secret",
    "password",
    "passwd",
    "credential",
    "private",
    "signing",
    "encryption",
];

fn has_strong_keyword(key: &str) -> bool {
    let lower = key.to_lowercase();
    STRONG_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

struct SensitiveAssignmentScanner;
impl SecretScanner for SensitiveAssignmentScanner {
    fn scan(&self, input: &str) -> Vec<SecretMatch> {
        let re = Regex::new(
            r"(?i)([A-Za-z0-9_]*(?:SECRET|PASSWORD|PASSWD|CREDENTIAL|PRIVATE|SIGNING|ENCRYPTION|TOKEN|API_KEY|AUTH_KEY)[A-Za-z0-9_]*)=(\S+)",
        )
        .unwrap();
        re.captures_iter(input)
            .filter_map(|cap| {
                let val = cap.get(2).unwrap();
                if val.as_str().starts_with('$') {
                    return None;
                }
                let key = cap.get(1).unwrap().as_str();
                // Weak keywords require a substantial value
                if !has_strong_keyword(key) && val.as_str().len() < 10 {
                    return None;
                }
                Some(SecretMatch {
                    start: val.start(),
                    end: val.end(),
                    label: classify_key(key),
                })
            })
            .collect()
    }
    fn priority(&self) -> u8 {
        10
    }
}
inventory::submit!(&SensitiveAssignmentScanner as &dyn SecretScanner);

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(input: &str) -> Vec<SecretMatch> {
        SensitiveAssignmentScanner.scan(input)
    }

    fn assert_detects(input: &str, msg: &str) {
        let matches = scan(input);
        assert!(!matches.is_empty(), "should detect: {msg} — input: {input}");
    }

    fn assert_ignores(input: &str, msg: &str) {
        let matches = scan(input);
        assert!(
            matches.is_empty(),
            "should NOT detect: {msg} — input: {input}",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 1. SECRET KEYWORD ASSIGNMENTS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_secret_equals_value() {
        assert_detects("SECRET=hunter2", "bare SECRET=value");
    }

    #[test]
    fn detects_aws_secret_access_key() {
        assert_detects(
            "AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "AWS secret access key assignment",
        );
    }

    #[test]
    fn detects_secret_key_assignment() {
        assert_detects("SECRET_KEY=s3krit_v4lue", "SECRET_KEY=value");
    }

    #[test]
    fn detects_client_secret() {
        assert_detects(
            "CLIENT_SECRET=abcdef123456",
            "CLIENT_SECRET (OAuth client secret)",
        );
    }

    #[test]
    fn detects_app_secret() {
        assert_detects("APP_SECRET=my-app-secret-value", "APP_SECRET=value");
    }

    #[test]
    fn detects_jwt_secret() {
        assert_detects("JWT_SECRET=supersecretkey123", "JWT_SECRET=value");
    }

    #[test]
    fn detects_api_secret() {
        assert_detects("API_SECRET=abc123def456", "API_SECRET=value");
    }

    #[test]
    fn detects_secret_token() {
        assert_detects("SECRET_TOKEN=tok_live_abc123", "SECRET_TOKEN=value");
    }

    #[test]
    fn detects_db_secret() {
        assert_detects("DB_SECRET=mydbsecret", "DB_SECRET=value");
    }

    // ──────────────────────────────────────────────────────────────
    // 2. PASSWORD KEYWORD ASSIGNMENTS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_password_equals_value() {
        assert_detects("PASSWORD=hunter2", "bare PASSWORD=value");
    }

    #[test]
    fn detects_db_password() {
        assert_detects("DB_PASSWORD=p@ssw0rd!", "DB_PASSWORD=value");
    }

    #[test]
    fn detects_mysql_password() {
        assert_detects("MYSQL_PASSWORD=root123", "MYSQL_PASSWORD=value");
    }

    #[test]
    fn detects_postgres_password() {
        assert_detects("POSTGRES_PASSWORD=pgpass", "POSTGRES_PASSWORD=value");
    }

    #[test]
    fn detects_redis_password() {
        assert_detects("REDIS_PASSWORD=redispass", "REDIS_PASSWORD=value");
    }

    #[test]
    fn detects_admin_password() {
        assert_detects("ADMIN_PASSWORD=admin123", "ADMIN_PASSWORD=value");
    }

    #[test]
    fn detects_root_password() {
        assert_detects("ROOT_PASSWORD=toor", "ROOT_PASSWORD=value");
    }

    #[test]
    fn detects_user_password() {
        assert_detects("USER_PASSWORD=welcome1", "USER_PASSWORD=value");
    }

    #[test]
    fn detects_password_prefix_key() {
        assert_detects("PASSWORD_FILE=/run/secrets/db", "PASSWORD as prefix in key");
    }

    // ──────────────────────────────────────────────────────────────
    // 3. PASSWD KEYWORD ASSIGNMENTS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_passwd_equals_value() {
        assert_detects("PASSWD=hunter2", "bare PASSWD=value");
    }

    #[test]
    fn detects_db_passwd() {
        assert_detects("DB_PASSWD=mydbpass", "DB_PASSWD=value");
    }

    #[test]
    fn detects_mysql_root_passwd() {
        assert_detects("MYSQL_ROOT_PASSWD=rootpw", "MYSQL_ROOT_PASSWD=value");
    }

    // ──────────────────────────────────────────────────────────────
    // 4. CASE INSENSITIVITY
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_lowercase_secret() {
        assert_detects("secret=hunter2", "lowercase secret=value");
    }

    #[test]
    fn detects_lowercase_password() {
        assert_detects("password=hunter2", "lowercase password=value");
    }

    #[test]
    fn detects_mixed_case_secret() {
        assert_detects("Secret_Key=abc123", "mixed case Secret_Key=value");
    }

    #[test]
    fn detects_mixed_case_password() {
        assert_detects("Password=abc123", "mixed case Password=value");
    }

    #[test]
    fn detects_all_caps_password() {
        assert_detects("PASSWORD=abc123", "all caps PASSWORD=value");
    }

    #[test]
    fn detects_camel_case_password() {
        // camelCase like `dbPassword=` may appear in config files
        assert_detects("dbPassword=secret123", "camelCase dbPassword=value");
    }

    #[test]
    fn detects_lowercase_passwd() {
        assert_detects("passwd=hunter2", "lowercase passwd=value");
    }

    // ──────────────────────────────────────────────────────────────
    // 5. REAL-WORLD EMBEDDING CONTEXTS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_in_shell_export() {
        assert_detects(
            "export DB_PASSWORD=hunter2",
            "password in shell export statement",
        );
    }

    #[test]
    fn detects_in_inline_env() {
        assert_detects(
            "DB_PASSWORD=hunter2 ./run.sh",
            "password in inline env var before command",
        );
    }

    #[test]
    fn detects_in_dotenv_file() {
        assert_detects("SECRET_KEY=abcdef123456", "secret in .env file");
    }

    #[test]
    fn detects_in_docker_env() {
        assert_detects(
            "ENV MYSQL_PASSWORD=rootpass",
            "password in Dockerfile ENV instruction",
        );
    }

    #[test]
    fn detects_in_docker_run_e_flag() {
        assert_detects(
            "docker run -e DB_PASSWORD=hunter2 myimage",
            "password in docker run -e",
        );
    }

    #[test]
    fn detects_in_docker_compose() {
        assert_detects(
            "      POSTGRES_PASSWORD=pgpass123",
            "password in docker-compose environment section",
        );
    }

    #[test]
    fn detects_in_k8s_manifest() {
        assert_detects(
            "  DB_PASSWORD=c2VjcmV0",
            "password in K8s env var (even if base64)",
        );
    }

    #[test]
    fn detects_in_ci_yaml() {
        assert_detects("  SECRET_KEY=abc123", "secret in CI/CD YAML config");
    }

    #[test]
    fn detects_in_systemd_env_file() {
        assert_detects("DB_PASSWORD=hunter2", "password in systemd EnvironmentFile");
    }

    #[test]
    fn detects_in_makefile() {
        assert_detects(
            "SECRET_KEY=mysecretkey make deploy",
            "secret in Makefile invocation",
        );
    }

    #[test]
    fn detects_in_terraform_var() {
        assert_detects(
            r#"TF_VAR_db_password=hunter2 terraform apply"#,
            "password in Terraform TF_VAR_ prefix",
        );
    }

    #[test]
    fn detects_in_ansible_extra_vars() {
        assert_detects(
            "ansible-playbook site.yml -e db_password=hunter2",
            "password in Ansible extra vars",
        );
    }

    #[test]
    fn detects_in_connection_string() {
        assert_detects(
            "DATABASE_PASSWORD=p@ss://special",
            "password with special chars in value",
        );
    }

    #[test]
    fn detects_in_proc_environ() {
        // /proc/<pid>/environ uses null separators
        assert_detects(
            "PATH=/usr/bin\0SECRET_KEY=leaked\0HOME=/root",
            "secret in /proc/PID/environ dump",
        );
    }

    #[test]
    fn detects_in_log_output() {
        assert_detects(
            "2024-01-15 10:30:00 [DEBUG] config: DB_PASSWORD=hunter2",
            "password leaked in log line",
        );
    }

    #[test]
    fn detects_in_error_message() {
        assert_detects(
            "Error: invalid credentials for SECRET_KEY=abc123xyz",
            "secret in error message",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 6. VALUE FORMATS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_value_with_special_characters() {
        assert_detects("PASSWORD=p@$$w0rd!#%^", "value with special characters");
    }

    #[test]
    fn detects_value_with_url_encoded_chars() {
        assert_detects(
            "PASSWORD=p%40ssw0rd%21",
            "value with URL-encoded characters",
        );
    }

    #[test]
    fn detects_single_char_value() {
        assert_detects("PASSWORD=x", "single character value");
    }

    #[test]
    fn detects_numeric_value() {
        assert_detects("PASSWORD=123456", "numeric-only value");
    }

    #[test]
    fn detects_base64_value() {
        assert_detects("SECRET=dGhpcyBpcyBhIHNlY3JldA==", "base64-encoded value");
    }

    #[test]
    fn detects_uuid_value() {
        assert_detects(
            "SECRET_KEY=550e8400-e29b-41d4-a716-446655440000",
            "UUID as value",
        );
    }

    #[test]
    fn detects_quoted_value() {
        // Quoted values: the quotes may be part of \S+
        assert_detects(r#"PASSWORD="hunter2""#, "double-quoted value");
    }

    #[test]
    fn detects_single_quoted_value() {
        assert_detects("PASSWORD='hunter2'", "single-quoted value");
    }

    // ──────────────────────────────────────────────────────────────
    // 7. LABEL CLASSIFICATION
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn labels_password_key_as_password() {
        let matches = scan("DB_PASSWORD=hunter2");
        assert_eq!(
            matches[0].label, "password",
            "PASSWORD key → password label"
        );
    }

    #[test]
    fn labels_passwd_key_as_password() {
        let matches = scan("DB_PASSWD=hunter2");
        assert_eq!(matches[0].label, "password", "PASSWD key → password label");
    }

    #[test]
    fn labels_secret_key_as_secret() {
        let matches = scan("SECRET_KEY=abc123");
        assert_eq!(matches[0].label, "secret", "SECRET key → secret label");
    }

    #[test]
    fn labels_aws_secret_as_secret() {
        let matches = scan("AWS_SECRET_ACCESS_KEY=abc123");
        assert_eq!(matches[0].label, "secret", "AWS_SECRET key → secret label");
    }

    #[test]
    fn labels_mixed_password_secret_by_password_priority() {
        // If key contains both PASSWORD and SECRET, classify_key checks
        // password/passwd first
        let matches = scan("SECRET_PASSWORD=abc123");
        assert_eq!(
            matches[0].label, "password",
            "key with both SECRET and PASSWORD → password label (password takes priority)",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 8. MATCH POSITION (only the VALUE is redacted, not the key)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn match_covers_only_value_not_key() {
        let input = "PASSWORD=hunter2";
        let matches = scan(input);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start, 9); // after "PASSWORD="
        assert_eq!(matches[0].end, 16); // end of "hunter2"
    }

    #[test]
    fn match_covers_only_value_with_prefix() {
        let input = "export DB_PASSWORD=s3cret";
        let matches = scan(input);
        assert_eq!(matches[0].start, 19); // after "export DB_PASSWORD="
        assert_eq!(matches[0].end, 25); // end of "s3cret"
    }

    #[test]
    fn match_covers_value_up_to_first_whitespace() {
        let input = "SECRET=value1 other=stuff";
        let matches = scan(input);
        assert_eq!(&input[matches[0].start..matches[0].end], "value1");
    }

    // ──────────────────────────────────────────────────────────────
    // 9. MULTIPLE ASSIGNMENTS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_multiple_secrets_on_separate_lines() {
        let input = "SECRET_KEY=abc123\nDB_PASSWORD=hunter2";
        let matches = scan(input);
        assert_eq!(matches.len(), 2, "should find both assignments");
    }

    #[test]
    fn detects_multiple_secrets_on_same_line() {
        let input = "SECRET=abc DB_PASSWORD=hunter2";
        let matches = scan(input);
        assert_eq!(matches.len(), 2, "should find both inline assignments");
    }

    #[test]
    fn detects_secret_and_password_together() {
        let input = "AWS_SECRET_ACCESS_KEY=abc123 DB_PASSWORD=hunter2";
        let matches = scan(input);
        assert_eq!(matches.len(), 2);
    }

    // ──────────────────────────────────────────────────────────────
    // 10. SHELL VARIABLE REFERENCES (should be ignored — no value)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn ignores_dollar_variable_reference() {
        assert_ignores("PASSWORD=$DB_PASS", "value is a $variable reference");
    }

    #[test]
    fn ignores_dollar_curly_variable() {
        assert_ignores("SECRET=${MY_SECRET}", "value is ${...} variable");
    }

    #[test]
    fn ignores_dollar_paren_command_sub() {
        assert_ignores(
            "PASSWORD=$(vault read secret/db)",
            "value is $(command) substitution",
        );
    }

    #[test]
    fn ignores_dollar_at_env() {
        assert_ignores("SECRET=$SECRET_FROM_VAULT", "value is $ENV_VAR reference");
    }

    // ──────────────────────────────────────────────────────────────
    // 11. TRUE NEGATIVES — things that should NOT be flagged
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn ignores_empty_input() {
        assert_ignores("", "empty input");
    }

    #[test]
    fn ignores_whitespace_only() {
        assert_ignores("   \n\t  ", "whitespace only");
    }

    #[test]
    fn ignores_normal_text() {
        assert_ignores(
            "The quick brown fox jumps over the lazy dog",
            "plain English text",
        );
    }

    #[test]
    fn ignores_key_without_secret_password_passwd() {
        assert_ignores(
            "API_KEY=abc123",
            "key is API_KEY, not SECRET/PASSWORD/PASSWD",
        );
    }

    #[test]
    fn ignores_token_assignment() {
        assert_ignores(
            "AUTH_TOKEN=abc123",
            "key is AUTH_TOKEN, no secret/password keyword",
        );
    }

    #[test]
    fn ignores_generic_key_value() {
        assert_ignores("DB_HOST=localhost", "DB_HOST — not sensitive");
    }

    #[test]
    fn ignores_path_assignment() {
        assert_ignores("PATH=/usr/local/bin:/usr/bin", "PATH variable");
    }

    #[test]
    fn ignores_port_assignment() {
        assert_ignores("DB_PORT=5432", "port number");
    }

    #[test]
    fn ignores_assignment_without_value() {
        assert_ignores("PASSWORD=", "empty value after =");
    }

    #[test]
    fn ignores_assignment_with_space_after_equals() {
        // `\S+` won't match if the value starts with a space
        assert_ignores("PASSWORD= hunter2", "space after = (value not adjacent)");
    }

    #[test]
    fn ignores_word_containing_secret_without_equals() {
        assert_ignores("the secret is out", "word 'secret' in prose, no assignment");
    }

    #[test]
    fn ignores_word_containing_password_without_equals() {
        assert_ignores(
            "please reset your password",
            "word 'password' in prose, no assignment",
        );
    }

    #[test]
    fn ignores_comment_about_passwords() {
        assert_ignores(
            "# PASSWORD is stored in vault",
            "comment mentioning PASSWORD without assignment",
        );
    }

    #[test]
    fn ignores_colon_separated_value() {
        // This scanner is for `=` assignments; colon-separated is for JSON scanner
        assert_ignores(
            "password: hunter2",
            "colon-separated assignment (handled by JSON scanner)",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 12. PRIORITY
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn priority_is_context_level() {
        let scanner = SensitiveAssignmentScanner;
        assert_eq!(
            scanner.priority(),
            10,
            "context-based scanner should have priority 10+",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 13. EDGE CASES / BOUNDARY
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn handles_very_long_value() {
        let value = "a".repeat(10_000);
        let input = format!("SECRET={value}");
        let matches = scan(&input);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].end - matches[0].start, 10_000);
    }

    #[test]
    fn handles_many_assignments_in_large_input() {
        let line = "SECRET_KEY=value123\n";
        let input = line.repeat(100);
        let matches = scan(&input);
        assert_eq!(matches.len(), 100);
    }

    #[test]
    fn handles_unicode_surrounding_assignment() {
        assert_detects(
            "日本語 SECRET=hunter2 中文",
            "assignment surrounded by CJK characters",
        );
    }

    #[test]
    fn handles_assignment_at_end_of_input() {
        assert_detects(
            "SECRET=hunter2",
            "assignment at end with no trailing newline",
        );
    }

    #[test]
    fn handles_null_bytes_in_input() {
        assert_detects(
            "FOO=bar\0SECRET=hunter2\0BAZ=qux",
            "assignment in null-separated /proc/environ",
        );
    }

    #[test]
    fn detects_key_with_numbers() {
        assert_detects("SECRET_KEY_2=value", "key with trailing number");
    }

    // ──────────────────────────────────────────────────────────────
    // 14. INTENTIONAL FALSE NEGATIVES — things worth detecting in
    //     the future but this scanner intentionally misses today
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn should_detect_api_key_assignment() {
        // API_KEY is extremely common and almost always sensitive.
        // Currently missed because API_KEY doesn't contain SECRET/PASSWORD/PASSWD.
        assert_detects(
            "API_KEY=sk-1234567890abcdef",
            "API_KEY= assignment — very common sensitive pattern",
        );
    }

    #[test]
    fn should_detect_access_token_assignment() {
        assert_detects(
            "ACCESS_TOKEN=xoxb-1234-5678-abcdef",
            "ACCESS_TOKEN= assignment — Slack/OAuth tokens",
        );
    }

    #[test]
    fn should_detect_private_key_inline() {
        assert_detects(
            "PRIVATE_KEY=-----BEGIN RSA PRIVATE KEY-----",
            "PRIVATE_KEY= assignment — RSA private key inline",
        );
    }

    #[test]
    fn should_detect_auth_key_assignment() {
        assert_detects(
            "AUTH_KEY=abc123secretvalue",
            "AUTH_KEY= assignment — authentication key",
        );
    }

    #[test]
    fn should_detect_encryption_key_assignment() {
        assert_detects(
            "ENCRYPTION_KEY=0123456789abcdef0123456789abcdef",
            "ENCRYPTION_KEY= assignment — encryption key material",
        );
    }

    #[test]
    fn should_detect_signing_key_assignment() {
        assert_detects(
            "SIGNING_KEY=whsec_abc123def456",
            "SIGNING_KEY= assignment — webhook signing key",
        );
    }

    #[test]
    fn should_detect_credentials_assignment() {
        assert_detects("CREDENTIALS=user:pass", "CREDENTIALS= assignment");
    }
}
