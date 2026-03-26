use regex::Regex;

use crate::core::secrets::engine::{SecretMatch, SecretScanner};

struct AwsKeyScanner;
impl SecretScanner for AwsKeyScanner {
    fn scan(&self, input: &str) -> Vec<SecretMatch> {
        let mut matches = Vec::new();

        // AWS access key IDs: AKIA (permanent) or ASIA (temporary STS)
        let key_id_re = Regex::new(r"A[KS]IA[A-Za-z0-9]{16}").unwrap();
        for m in key_id_re.find_iter(input) {
            // Reject if preceded by an uppercase letter (e.g. "FAKIA...")
            // Lowercase/digits/symbols are fine — they indicate the key is
            // embedded in noise or after a delimiter, not part of a longer identifier.
            if m.start() > 0
                && input.as_bytes()[m.start() - 1].is_ascii_uppercase()
            {
                continue;
            }
            matches.push(SecretMatch {
                start: m.start(),
                end: m.end(),
                label: "aws_key".into(),
            });
        }

        // AWS secret access keys: exactly 40 chars of [A-Za-z0-9/+]
        let secret_re = Regex::new(r"[A-Za-z0-9/+]{40}").unwrap();
        for m in secret_re.find_iter(input) {
            // Must not be part of a longer token of the same char class
            let left_ok = m.start() == 0 || {
                let b = input.as_bytes()[m.start() - 1];
                !(b.is_ascii_alphanumeric() || b == b'/' || b == b'+')
            };
            let right_ok = m.end() >= input.len() || {
                let b = input.as_bytes()[m.end()];
                !(b.is_ascii_alphanumeric() || b == b'/' || b == b'+')
            };
            if !left_ok || !right_ok {
                continue;
            }
            // Skip if overlapping with an already-detected key ID
            if matches
                .iter()
                .any(|e| e.start < m.end() && m.start() < e.end)
            {
                continue;
            }
            matches.push(SecretMatch {
                start: m.start(),
                end: m.end(),
                label: "aws_key".into(),
            });
        }

        matches
    }
}
inventory::submit!(&AwsKeyScanner as &dyn SecretScanner);

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(input: &str) -> Vec<SecretMatch> {
        AwsKeyScanner.scan(input)
    }

    fn assert_detects(input: &str, msg: &str) {
        let matches = scan(input);
        assert!(!matches.is_empty(), "should detect: {msg} — input: {input}");
    }

    fn assert_ignores(input: &str, msg: &str) {
        let matches = scan(input);
        assert!(matches.is_empty(), "should NOT detect: {msg} — input: {input}");
    }

    // ──────────────────────────────────────────────────────────────
    // 1. ACCESS KEY IDs (AKIA prefix — long-lived IAM credentials)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_standard_akia_access_key() {
        assert_detects("AKIAIOSFODNN7EXAMPLE", "standard AWS access key ID");
    }

    #[test]
    fn detects_akia_key_all_uppercase_letters() {
        assert_detects("AKIAZZZZZZZZZZZZZZZZ", "AKIA key with all uppercase letters");
    }

    #[test]
    fn detects_akia_key_all_digits() {
        assert_detects("AKIA1234567890123456", "AKIA key with all digits after prefix");
    }

    #[test]
    fn detects_akia_key_mixed_alphanum() {
        assert_detects("AKIA0A1B2C3D4E5F6G7H", "AKIA key with mixed alphanumeric");
    }

    // ──────────────────────────────────────────────────────────────
    // 2. TEMPORARY CREDENTIALS (ASIA prefix — STS tokens)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_asia_temporary_access_key() {
        assert_detects("ASIAXYZ12345ABCDEFGH", "temporary STS access key (ASIA)");
    }

    #[test]
    fn detects_asia_key_all_digits() {
        assert_detects("ASIA1234567890123456", "ASIA key with all digits");
    }

    // ──────────────────────────────────────────────────────────────
    // 3. SECRET ACCESS KEYS (40-char base64-ish companion secret)
    //    These are the actual secret, not just the key ID.
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_aws_secret_access_key_40_char() {
        // The secret access key that accompanies every key ID — arguably
        // MORE dangerous than the key ID alone.
        assert_detects(
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "AWS secret access key (40-char)",
        );
    }

    #[test]
    fn detects_secret_access_key_with_plus_and_slash() {
        assert_detects(
            "je7MtGbClwBF/2Zp9Utk/h3yCo8nvbEXAMPLEKEY",
            "secret access key containing + and / characters",
        );
    }

    #[test]
    fn detects_secret_access_key_standalone() {
        // No key ID present — just the 40-char secret by itself
        assert_detects(
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "standalone secret access key without accompanying key ID",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 4. REAL-WORLD EMBEDDING CONTEXTS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_key_in_env_var_export() {
        assert_detects(
            "export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE",
            "key in shell export",
        );
    }

    #[test]
    fn detects_key_in_env_var_inline() {
        assert_detects(
            "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE aws s3 ls",
            "key in inline env var",
        );
    }

    #[test]
    fn detects_secret_in_env_var_export() {
        assert_detects(
            "export AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "secret access key in shell export",
        );
    }

    #[test]
    fn detects_key_in_double_quoted_string() {
        assert_detects(
            r#"aws_access_key_id = "AKIAIOSFODNN7EXAMPLE""#,
            "key in double-quoted string",
        );
    }

    #[test]
    fn detects_key_in_single_quoted_string() {
        assert_detects(
            "aws_access_key_id = 'AKIAIOSFODNN7EXAMPLE'",
            "key in single-quoted string",
        );
    }

    #[test]
    fn detects_key_in_json_object() {
        assert_detects(
            r#"{"AccessKeyId": "AKIAIOSFODNN7EXAMPLE", "SecretAccessKey": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"}"#,
            "key in JSON object",
        );
    }

    #[test]
    fn detects_key_in_yaml_config() {
        assert_detects(
            "aws_access_key_id: AKIAIOSFODNN7EXAMPLE",
            "key in YAML config",
        );
    }

    #[test]
    fn detects_key_in_terraform_block() {
        assert_detects(
            r#"access_key = "AKIAIOSFODNN7EXAMPLE""#,
            "key in Terraform provider block",
        );
    }

    #[test]
    fn detects_key_in_docker_env() {
        assert_detects(
            "ENV AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE",
            "key in Dockerfile ENV",
        );
    }

    #[test]
    fn detects_key_in_docker_run_flag() {
        assert_detects(
            "docker run -e AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE myimage",
            "key in docker run -e flag",
        );
    }

    #[test]
    fn detects_key_in_aws_credentials_file_format() {
        let input = "[default]\naws_access_key_id = AKIAIOSFODNN7EXAMPLE\naws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
        assert_detects(input, "key in ~/.aws/credentials format");
    }

    #[test]
    fn detects_key_in_url_query_param() {
        assert_detects(
            "https://example.com?AWSAccessKeyId=AKIAIOSFODNN7EXAMPLE&Expires=123",
            "key in presigned URL query param",
        );
    }

    #[test]
    fn detects_key_in_xml_response() {
        assert_detects(
            "<AccessKeyId>AKIAIOSFODNN7EXAMPLE</AccessKeyId>",
            "key in XML (STS GetSessionToken response)",
        );
    }

    #[test]
    fn detects_key_in_csv_export() {
        assert_detects(
            "user1,AKIAIOSFODNN7EXAMPLE,wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY,true",
            "key in CSV (IAM credential report export)",
        );
    }

    #[test]
    fn detects_key_in_log_line() {
        assert_detects(
            "2024-01-15T10:30:00Z INFO caller=AKIAIOSFODNN7EXAMPLE action=AssumeRole",
            "key leaked into application log line",
        );
    }

    #[test]
    fn detects_key_in_cli_flag() {
        assert_detects(
            "aws configure set aws_access_key_id AKIAIOSFODNN7EXAMPLE",
            "key passed as CLI argument",
        );
    }

    #[test]
    fn detects_key_in_github_actions_yaml() {
        assert_detects(
            "  AWS_ACCESS_KEY_ID: AKIAIOSFODNN7EXAMPLE",
            "key hardcoded in GitHub Actions YAML",
        );
    }

    #[test]
    fn detects_key_in_python_source() {
        assert_detects(
            r#"client = boto3.client("s3", aws_access_key_id="AKIAIOSFODNN7EXAMPLE")"#,
            "key in Python boto3 call",
        );
    }

    #[test]
    fn detects_key_in_java_source() {
        assert_detects(
            r#"BasicAWSCredentials creds = new BasicAWSCredentials("AKIAIOSFODNN7EXAMPLE", secret);"#,
            "key in Java AWS SDK code",
        );
    }

    #[test]
    fn detects_key_in_connection_string() {
        assert_detects(
            "s3://AKIAIOSFODNN7EXAMPLE:wJalrXUtnFEMI%2FK7MDENG%2FbPxRfiCYEXAMPLEKEY@bucket/path",
            "key in S3 connection string URI",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 5. MULTIPLE SECRETS IN ONE INPUT
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_multiple_distinct_keys() {
        let input = "key1=AKIAIOSFODNN7EXAMPLE key2=AKIAI99999999999AAAA";
        let matches = scan(input);
        assert_eq!(matches.len(), 2, "should find two distinct access key IDs");
    }

    #[test]
    fn detects_both_akia_and_asia_in_same_input() {
        let input = "main=AKIAIOSFODNN7EXAMPLE temp=ASIAXYZ12345ABCDEFGH";
        let matches = scan(input);
        assert_eq!(matches.len(), 2, "should find both AKIA and ASIA keys");
    }

    #[test]
    fn detects_key_id_and_secret_key_pair() {
        let input = "AKIAIOSFODNN7EXAMPLE:wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
        // At minimum the key ID must be detected; ideally both are flagged
        let matches = scan(input);
        assert!(
            !matches.is_empty(),
            "should detect at least the key ID in a key:secret pair",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 6. MATCH METADATA (position, label)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn match_start_and_end_are_correct_for_key_at_start() {
        let matches = scan("AKIAIOSFODNN7EXAMPLE rest");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start, 0);
        assert_eq!(matches[0].end, 20); // AKIA + 16 = 20 chars
    }

    #[test]
    fn match_start_and_end_are_correct_for_key_at_offset() {
        let matches = scan("prefix AKIAIOSFODNN7EXAMPLE suffix");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start, 7);
        assert_eq!(matches[0].end, 27);
    }

    #[test]
    fn match_label_is_aws_key() {
        let matches = scan("AKIAIOSFODNN7EXAMPLE");
        assert_eq!(matches[0].label, "aws_key");
    }

    // ──────────────────────────────────────────────────────────────
    // 7. FALSE NEGATIVES — near-miss formats that SHOULD be caught
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_key_with_lowercase_suffix_if_present_in_real_leak() {
        // Real AWS keys are uppercase, but a case-mangled log or
        // base64-decoded dump might lowercase them. A robust scanner
        // should still flag these as suspicious.
        // This is aspirational — current impl may fail this.
        assert_detects(
            "AKIAIOSFODNN7example",
            "key with lowercase chars after prefix (case-mangled leak)",
        );
    }

    #[test]
    fn detects_key_surrounded_by_whitespace_only() {
        assert_detects("   AKIAIOSFODNN7EXAMPLE   ", "key with surrounding whitespace");
    }

    #[test]
    fn detects_key_at_end_of_input_no_trailing() {
        assert_detects("something AKIAIOSFODNN7EXAMPLE", "key at end of input");
    }

    #[test]
    fn detects_key_on_its_own_line() {
        assert_detects(
            "line1\nAKIAIOSFODNN7EXAMPLE\nline3",
            "key alone on a line",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 8. TRUE NEGATIVES — things that should NOT be flagged
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
    fn ignores_env_var_reference_without_value() {
        assert_ignores("echo $AWS_ACCESS_KEY_ID", "shell variable reference (no value)");
    }

    #[test]
    fn detects_placeholder_akia_with_x_chars() {
        // AKIAXXXXXXXXXXXXXXXX is structurally a valid key — better to
        // over-detect and let a human dismiss it than miss a real key.
        assert_detects(
            "AKIAXXXXXXXXXXXXXXXX",
            "placeholder AKIA key with all X chars — still structurally valid",
        );
    }

    #[test]
    fn ignores_akia_prefix_too_short() {
        // Only 15 chars after AKIA — not a valid key
        assert_ignores("AKIAIOSFODNN7EXAMPL", "AKIA prefix + only 15 chars (too short)");
    }

    #[test]
    fn ignores_partial_prefix_aki() {
        assert_ignores("AKIIOSFODNN7EXAMPLE1", "AKI without the A — not AKIA prefix");
    }

    #[test]
    fn ignores_wrong_prefix_abia() {
        assert_ignores("ABIAIOSFODNN7EXAMPLE", "ABIA prefix — not a valid AWS key prefix");
    }

    #[test]
    fn ignores_wrong_prefix_acia() {
        assert_ignores("ACIAIOSFODNN7EXAMPLE", "ACIA prefix — not a valid AWS key prefix");
    }

    #[test]
    fn ignores_wrong_prefix_adia() {
        assert_ignores("ADIAIOSFODNN7EXAMPLE", "ADIA prefix — not a valid AWS key prefix");
    }

    #[test]
    fn ignores_lowercase_akia_prefix() {
        assert_ignores("akiaIOSFODNN7EXAMPLE", "lowercase akia prefix");
    }

    #[test]
    fn ignores_string_that_contains_akia_not_at_boundary() {
        // "FAKIA..." — AKIA appears but not at the start of the token
        assert_ignores(
            "FAKIAIOSFODNN7EXAMPLE",
            "AKIA embedded after F — not at token start",
        );
    }

    #[test]
    fn ignores_aws_arn_format() {
        assert_ignores(
            "arn:aws:iam::123456789012:user/johndoe",
            "AWS ARN — not a secret",
        );
    }

    #[test]
    fn ignores_aws_account_id() {
        assert_ignores("123456789012", "12-digit AWS account ID — not a secret");
    }

    #[test]
    fn ignores_generic_base64_string() {
        assert_ignores(
            "dGhpcyBpcyBhIHRlc3Q=",
            "generic base64 that happens to be 20+ chars",
        );
    }

    #[test]
    fn ignores_uuid() {
        assert_ignores(
            "550e8400-e29b-41d4-a716-446655440000",
            "UUID — not an AWS key",
        );
    }

    #[test]
    fn ignores_hex_hash() {
        assert_ignores(
            "a3f2b8c1d4e5f67890abcdef12345678",
            "hex hash — not an AWS key",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 9. BOUNDARY / EDGE CASES
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn does_not_over_match_into_longer_alphanumeric_string() {
        // If key is embedded in a 30-char alphanumeric blob, the scanner
        // should match exactly 20 chars (or the secret-key length), not more.
        let input = "AKIAIOSFODNN7EXAMPLEXYZ";
        let matches = scan(input);
        assert!(!matches.is_empty(), "should still detect the key");
        // The match should cover exactly the 20-char key ID
        let m = &matches[0];
        assert_eq!(
            m.end - m.start,
            20,
            "match should be exactly 20 chars for a key ID",
        );
    }

    #[test]
    fn handles_key_adjacent_to_special_chars() {
        assert_detects("(AKIAIOSFODNN7EXAMPLE)", "key in parentheses");
        assert_detects("[AKIAIOSFODNN7EXAMPLE]", "key in brackets");
        assert_detects("<AKIAIOSFODNN7EXAMPLE>", "key in angle brackets");
        assert_detects("\"AKIAIOSFODNN7EXAMPLE\"", "key in double quotes");
        assert_detects("'AKIAIOSFODNN7EXAMPLE'", "key in single quotes");
        assert_detects("`AKIAIOSFODNN7EXAMPLE`", "key in backticks");
    }

    #[test]
    fn handles_key_next_to_equals_sign() {
        assert_detects("KEY=AKIAIOSFODNN7EXAMPLE", "key after = with no space");
    }

    #[test]
    fn handles_key_after_colon_space() {
        assert_detects("key: AKIAIOSFODNN7EXAMPLE", "key after colon-space (YAML)");
    }

    #[test]
    fn handles_unicode_surrounding_key() {
        assert_detects(
            "日本語AKIAIOSFODNN7EXAMPLE中文",
            "key surrounded by CJK characters",
        );
    }

    #[test]
    fn handles_emoji_surrounding_key() {
        assert_detects(
            "🔑AKIAIOSFODNN7EXAMPLE🔑",
            "key surrounded by emoji",
        );
    }

    #[test]
    fn handles_very_long_input_with_embedded_key() {
        let padding = "x".repeat(10_000);
        let input = format!("{padding}AKIAIOSFODNN7EXAMPLE{padding}");
        let matches = scan(&input);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start, 10_000);
    }

    #[test]
    fn handles_many_keys_in_large_input() {
        let line = "key=AKIAIOSFODNN7EXAMPLE\n";
        let input = line.repeat(100);
        let matches = scan(&input);
        assert_eq!(matches.len(), 100, "should find one key per line × 100 lines");
    }

    #[test]
    fn handles_key_with_newline_immediately_after() {
        assert_detects("AKIAIOSFODNN7EXAMPLE\n", "key followed by newline");
    }

    #[test]
    fn handles_key_with_null_byte_context() {
        assert_detects(
            "foo\0AKIAIOSFODNN7EXAMPLE\0bar",
            "key surrounded by null bytes (binary context)",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 10. OBFUSCATION / EVASION ATTEMPTS the scanner SHOULD resist
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_key_with_zero_width_spaces_stripped() {
        // If someone inserts zero-width spaces, after stripping them
        // the key should still be detectable. Test the raw key first.
        // (The scanner may not strip unicode — but the key with ZWS
        // removed should still match.)
        let key_with_zws = "AKIA\u{200B}IOSFODNN7EXAMPLE";
        let cleaned: String = key_with_zws.replace('\u{200B}', "");
        assert_detects(&cleaned, "key after stripping zero-width spaces");
    }

    // ──────────────────────────────────────────────────────────────
    // 11. REGRESSION: the AWS documentation example keys
    //     https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_access-keys.html
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_aws_docs_example_access_key() {
        assert_detects("AKIAIOSFODNN7EXAMPLE", "AWS docs example access key ID");
    }

    #[test]
    fn detects_aws_docs_example_secret_key() {
        assert_detects(
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "AWS docs example secret access key",
        );
    }
}
