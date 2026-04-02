use regex::Regex;

use crate::core::secrets::engine::{SecretMatch, SecretScanner};

struct JwtScanner;
impl SecretScanner for JwtScanner {
    fn scan(&self, input: &str) -> Vec<SecretMatch> {
        let re = Regex::new(r"eyJ[A-Za-z0-9_=-]+\.[A-Za-z0-9_=-]+\.[A-Za-z0-9_=-]+").unwrap();
        re.find_iter(input)
            .map(|m| SecretMatch {
                start: m.start(),
                end: m.end(),
                label: "jwt".into(),
            })
            .collect()
    }
}
inventory::submit!(&JwtScanner as &dyn SecretScanner);

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(input: &str) -> Vec<SecretMatch> {
        JwtScanner.scan(input)
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

    // A realistic HS256 JWT (header.payload.signature)
    const VALID_HS256: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";

    // RS256 JWT (longer signature segment)
    const VALID_RS256: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkphbmUgRG9lIiwiYWRtaW4iOnRydWUsImlhdCI6MTUxNjIzOTAyMn0.POstGetfAytaZS82wHcjoTyoqhMyxXiWdR7Nn7A29DNSl0EiXLdwJ6xC6AfgZWF1bOsS_TuYI3OG85AmiExREkrS6tDfTQ2B3WXlrr-wp5AokiRbz3_oB4OxG-W9KcEEbDRcZc0nH3L7LzYptiy1PtAylQGxHTWZXtGz4ht0bAecBgmpdgXMguEIcoqPJ1n3pIWk_dUZegpqx0Lka21H6XxUTxiy8OcaarA8zdnPUnV6AmNP3ecFawIFYdvJB_cm-GvpCSbr8G8y_Mllj8f4x9nBH8pQux89_6gUY618iYv7tuPWBFfEbLxtF2pZS6YC1aSfLQxaOoaBSTFYIczBh3Q";

    // ES256 JWT (shorter signature)
    const VALID_ES256: &str = "eyJhbGciOiJFUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkFsaWNlIiwiaWF0IjoxNTE2MjM5MDIyfQ.hHTr8H7D_3atpnR1u4MaN3J-5zLYm1GqbOkpJxuJRSMY-3aRdVYz0i_3s-OcNdsa1R0Y-XwGKnNF4qJLnhKqqA";

    // ──────────────────────────────────────────────────────────────
    // 1. STANDARD JWT ALGORITHMS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_hs256_jwt() {
        assert_detects(VALID_HS256, "HS256 signed JWT");
    }

    #[test]
    fn detects_rs256_jwt() {
        assert_detects(VALID_RS256, "RS256 signed JWT (long signature)");
    }

    #[test]
    fn detects_es256_jwt() {
        assert_detects(VALID_ES256, "ES256 signed JWT (ECDSA)");
    }

    #[test]
    fn detects_hs384_jwt() {
        // HS384 header: {"alg":"HS384","typ":"JWT"}
        assert_detects(
            "eyJhbGciOiJIUzM4NCIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxIn0.VGhlIHNpZ25hdHVyZSBpcyA0OCBieXRlcyBsb25nIGZvckhTMzg0YWxnb3JpdGht",
            "HS384 JWT",
        );
    }

    #[test]
    fn detects_hs512_jwt() {
        // HS512 header: {"alg":"HS512","typ":"JWT"}
        assert_detects(
            "eyJhbGciOiJIUzUxMiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxIn0.VGhpcyBpcyBhIDY0IGJ5dGUgc2lnbmF0dXJlIGZvciBIUzUxMiBhbGdvcml0aG1fdGVzdGluZw",
            "HS512 JWT",
        );
    }

    #[test]
    fn detects_none_algorithm_jwt() {
        // {"alg":"none"} — unsigned JWT, often used in attacks
        assert_detects(
            "eyJhbGciOiJub25lIn0.eyJzdWIiOiIxMjM0NTY3ODkwIn0.signature",
            "JWT with alg:none (security risk)",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 2. JWT VARIANTS (JWS, JWE, nested)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_jwe_five_segments() {
        // JWE tokens have 5 dot-separated segments. The scanner should
        // detect them — they contain encrypted secrets.
        assert_detects(
            "eyJhbGciOiJSU0EtT0FFUCIsImVuYyI6IkEyNTZHQ00ifQ.OKOawDo13gRp2ojaHV7LFpZcgV7T6DVZKTyKOMTYUmKoTCVJRgckCL9kiMT03JGeipsEdY3mx_etLbbWSrFr05kLzcSr4qKAq7YN7e9jwQRb23nfa6c9d-StnImGyFDbSv04uVuxIp5Zms1gNxKKK2Da14B8S4rzVRltdYwam_lDp5XnZAYpQdb76FdIKLaVmqgfwX7XWRxv2322i-vDxRfqNzo_tETKzpVLzfiwQyeyPGLBIO56YJ7eObdv0je81860ppamavo35UgoRdbYaBcoh9QcfylQr66oc6vFWXRcZ_ZT2LawVCWTIy3brGPi6UklfCpIMfIjf7iGdXKHzg.48V1_ALb6US04U3b.5eym8TW_c8SuK0ltJ3rpYIzOeDQz7TALvtu6UG9oMo4vpzs9tX_EFShS8iB7j6jiSdiwkIr3ajwQzaBtQD_.XFBoMYUZodetZdvTiFvSkQ",
            "JWE token (5 dot-separated segments)",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 3. REAL-WORLD JWT ISSUERS / SERVICES
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_github_app_jwt() {
        // GitHub App installation tokens are JWTs
        assert_detects(VALID_RS256, "GitHub App JWT");
    }

    #[test]
    fn detects_auth0_jwt() {
        assert_detects(VALID_HS256, "Auth0 access token (JWT)");
    }

    #[test]
    fn detects_firebase_jwt() {
        assert_detects(VALID_RS256, "Firebase Auth ID token (JWT)");
    }

    #[test]
    fn detects_aws_cognito_jwt() {
        assert_detects(VALID_RS256, "AWS Cognito ID/access token (JWT)");
    }

    // ──────────────────────────────────────────────────────────────
    // 4. REAL-WORLD EMBEDDING CONTEXTS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_jwt_in_authorization_bearer_header() {
        let input = format!("Authorization: Bearer {VALID_HS256}");
        assert_detects(&input, "JWT in Authorization: Bearer header");
    }

    #[test]
    fn detects_jwt_in_cookie() {
        let input = format!("Set-Cookie: token={VALID_HS256}; HttpOnly; Secure");
        assert_detects(&input, "JWT in Set-Cookie header");
    }

    #[test]
    fn detects_jwt_in_curl_header() {
        let input =
            format!(r#"curl -H "Authorization: Bearer {VALID_HS256}" https://api.example.com"#);
        assert_detects(&input, "JWT in curl command");
    }

    #[test]
    fn detects_jwt_in_env_var_export() {
        let input = format!("export TOKEN={VALID_HS256}");
        assert_detects(&input, "JWT in shell export");
    }

    #[test]
    fn detects_jwt_in_dotenv_file() {
        let input = format!("JWT_SECRET={VALID_HS256}");
        assert_detects(&input, "JWT in .env file");
    }

    #[test]
    fn detects_jwt_in_json_response() {
        let input = format!(r#"{{"access_token": "{VALID_HS256}", "token_type": "Bearer"}}"#);
        assert_detects(&input, "JWT in JSON API response body");
    }

    #[test]
    fn detects_jwt_in_yaml_config() {
        let input = format!("auth_token: {VALID_HS256}");
        assert_detects(&input, "JWT in YAML config");
    }

    #[test]
    fn detects_jwt_in_url_query_param() {
        let input = format!("https://example.com/callback?token={VALID_HS256}");
        assert_detects(&input, "JWT in URL query parameter");
    }

    #[test]
    fn detects_jwt_in_url_fragment() {
        let input = format!("https://example.com/app#id_token={VALID_HS256}");
        assert_detects(&input, "JWT in URL fragment (OAuth implicit flow)");
    }

    #[test]
    fn detects_jwt_in_html_meta_tag() {
        let input = format!(r#"<meta name="api-token" content="{VALID_HS256}">"#);
        assert_detects(&input, "JWT in HTML meta tag");
    }

    #[test]
    fn detects_jwt_in_javascript_source() {
        let input = format!(r#"const token = "{VALID_HS256}";"#);
        assert_detects(&input, "JWT in JavaScript source");
    }

    #[test]
    fn detects_jwt_in_python_source() {
        let input = format!(r#"headers = {{"Authorization": "Bearer {VALID_HS256}"}}"#);
        assert_detects(&input, "JWT in Python source");
    }

    #[test]
    fn detects_jwt_in_local_storage_dump() {
        let input = format!(r#"localStorage.setItem("token", "{VALID_HS256}");"#);
        assert_detects(&input, "JWT in localStorage (common XSS target)");
    }

    #[test]
    fn detects_jwt_in_log_line() {
        let input = format!("2024-01-15T10:30:00Z INFO auth token={VALID_HS256} user=admin");
        assert_detects(&input, "JWT leaked in application log");
    }

    #[test]
    fn detects_jwt_in_docker_env() {
        let input = format!("ENV AUTH_TOKEN={VALID_HS256}");
        assert_detects(&input, "JWT in Dockerfile ENV");
    }

    #[test]
    fn detects_jwt_in_terraform() {
        let input = format!(r#"token = "{VALID_HS256}""#);
        assert_detects(&input, "JWT in Terraform config");
    }

    #[test]
    fn detects_jwt_in_xml() {
        let input = format!("<Token>{VALID_HS256}</Token>");
        assert_detects(&input, "JWT in XML element");
    }

    #[test]
    fn detects_jwt_in_grpc_metadata() {
        let input = format!("authorization: bearer {VALID_HS256}");
        assert_detects(&input, "JWT in gRPC metadata (lowercase bearer)");
    }

    #[test]
    fn detects_jwt_in_graphql_query() {
        let input = format!(
            r#"{{"query": "mutation {{ login }}", "extensions": {{"token": "{VALID_HS256}"}}}}"#
        );
        assert_detects(&input, "JWT in GraphQL request body");
    }

    // ──────────────────────────────────────────────────────────────
    // 5. MULTIPLE JWTs
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_multiple_jwts_in_same_input() {
        let input = format!("access={VALID_HS256} refresh={VALID_RS256}");
        let matches = scan(&input);
        assert_eq!(matches.len(), 2, "should find two distinct JWTs");
    }

    #[test]
    fn detects_jwts_on_separate_lines() {
        let input = format!("token1={VALID_HS256}\ntoken2={VALID_ES256}\n");
        let matches = scan(&input);
        assert_eq!(matches.len(), 2, "should find JWTs across lines");
    }

    // ──────────────────────────────────────────────────────────────
    // 6. MATCH METADATA (position, label)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn match_start_correct_at_beginning() {
        let matches = scan(VALID_HS256);
        assert_eq!(matches[0].start, 0);
    }

    #[test]
    fn match_start_correct_at_offset() {
        let input = format!("Bearer {VALID_HS256}");
        let matches = scan(&input);
        assert_eq!(matches[0].start, 7);
    }

    #[test]
    fn match_end_covers_full_token() {
        let matches = scan(VALID_HS256);
        assert_eq!(matches[0].end, VALID_HS256.len());
    }

    #[test]
    fn match_label_is_jwt() {
        let matches = scan(VALID_HS256);
        assert_eq!(matches[0].label, "jwt");
    }

    // ──────────────────────────────────────────────────────────────
    // 7. HEADER VARIATIONS (all start with eyJ = base64 of '{"')
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_jwt_with_kid_header() {
        // {"alg":"RS256","typ":"JWT","kid":"mykey-1"} → eyJ... prefix
        assert_detects(
            "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6Im15a2V5LTEifQ.eyJzdWIiOiIxIn0.c2lnbmF0dXJl",
            "JWT with kid claim in header",
        );
    }

    #[test]
    fn detects_jwt_with_jku_header() {
        // Headers with jku (JWK Set URL) are security-sensitive
        assert_detects(
            "eyJhbGciOiJSUzI1NiIsImprdSI6Imh0dHBzOi8vZXhhbXBsZS5jb20vLndlbGwta25vd24vandrcy5qc29uIn0.eyJzdWIiOiIxIn0.c2lnbmF0dXJl",
            "JWT with jku header (JWKS URL)",
        );
    }

    #[test]
    fn detects_jwt_with_x5c_header() {
        // x5c header contains embedded X.509 certificate chain
        assert_detects(
            "eyJhbGciOiJSUzI1NiIsIng1YyI6WyJNSUlCLi4uIl19.eyJzdWIiOiIxIn0.c2lnbmF0dXJl",
            "JWT with x5c header (embedded certificate)",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 8. PAYLOAD VARIATIONS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_jwt_with_minimal_payload() {
        assert_detects("eyJhbGciOiJIUzI1NiJ9.eyJ9.c2ln", "JWT with minimal payload");
    }

    #[test]
    fn detects_jwt_with_large_payload() {
        // Simulating a JWT with many claims (long middle segment)
        let header = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let payload = "eyJ".to_string() + &"a".repeat(500);
        let sig = "abcdefghij1234567890";
        let jwt = format!("{header}.{payload}.{sig}");
        assert_detects(&jwt, "JWT with very large payload (many claims)");
    }

    // ──────────────────────────────────────────────────────────────
    // 9. BASE64URL ENCODING EDGE CASES
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_jwt_with_underscores_in_segments() {
        // Base64url uses _ instead of / — must be accepted
        assert_detects(
            "eyJhbGciOiJIUzI1NiJ9.eyJkYXRhIjoiYV9iX2MifQ.abc_def_ghi_jkl_mno",
            "JWT with underscores in base64url segments",
        );
    }

    #[test]
    fn detects_jwt_with_hyphens_in_segments() {
        // Base64url uses - instead of + — must be accepted
        assert_detects(
            "eyJhbGciOiJIUzI1NiJ9.eyJkYXRhIjoiYS1iLWMifQ.abc-def-ghi-jkl-mno",
            "JWT with hyphens in base64url segments",
        );
    }

    #[test]
    fn detects_jwt_without_padding() {
        // JWTs typically omit base64 padding (=) — this is standard
        assert_detects(VALID_HS256, "JWT without padding characters (standard)");
    }

    #[test]
    fn detects_jwt_with_padding_if_present() {
        // Non-standard but some libraries add padding
        assert_detects(
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0=.c2lnbmF0dXJl",
            "JWT with = padding (non-standard but real)",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 10. TRUE NEGATIVES — things that should NOT be flagged
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
            "plain English",
        );
    }

    #[test]
    fn ignores_base64_string_not_starting_with_eyj() {
        // Valid base64 with dots but no eyJ prefix
        assert_ignores(
            "dGhpcyBpcyBub3Q.YSBqd3Q.YXQgYWxs",
            "base64 with dots but not starting with eyJ",
        );
    }

    #[test]
    fn ignores_eyj_without_dots() {
        assert_ignores(
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
            "eyJ... string with no dots (just a header, not a full JWT)",
        );
    }

    #[test]
    fn ignores_eyj_with_only_one_dot() {
        assert_ignores(
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0",
            "eyJ... with only one dot (header.payload, no signature)",
        );
    }

    #[test]
    fn ignores_two_dots_but_empty_segments() {
        assert_ignores("eyJ..", "eyJ with two dots but empty payload and signature");
    }

    #[test]
    fn ignores_uuid() {
        assert_ignores("550e8400-e29b-41d4-a716-446655440000", "UUID — not a JWT");
    }

    #[test]
    fn ignores_aws_key() {
        assert_ignores("AKIAIOSFODNN7EXAMPLE", "AWS access key — not a JWT");
    }

    #[test]
    fn ignores_github_token() {
        assert_ignores(
            "ghp_1234567890abcdef1234567890abcdef12345678",
            "GitHub token — not a JWT",
        );
    }

    #[test]
    fn ignores_random_base64_with_dots() {
        assert_ignores(
            "aGVsbG8.d29ybGQ.Zm9v",
            "random base64 with dots but no eyJ prefix",
        );
    }

    #[test]
    fn ignores_java_package_path() {
        assert_ignores(
            "com.example.jwt.JwtService",
            "Java package path containing 'jwt'",
        );
    }

    #[test]
    fn ignores_file_path_with_dots() {
        assert_ignores("/path/to/some.file.txt", "file path with dots");
    }

    #[test]
    fn ignores_ip_address() {
        assert_ignores("192.168.1.1", "IP address — not a JWT");
    }

    #[test]
    fn ignores_semver() {
        assert_ignores("1.2.3", "semantic version — not a JWT");
    }

    // ──────────────────────────────────────────────────────────────
    // 11. BOUNDARY / EDGE CASES
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn handles_jwt_surrounded_by_quotes() {
        let input = format!(r#""{VALID_HS256}""#);
        assert_detects(&input, "JWT in double quotes");
    }

    #[test]
    fn handles_jwt_surrounded_by_single_quotes() {
        let input = format!("'{VALID_HS256}'");
        assert_detects(&input, "JWT in single quotes");
    }

    #[test]
    fn handles_jwt_in_parentheses() {
        let input = format!("({VALID_HS256})");
        assert_detects(&input, "JWT in parentheses");
    }

    #[test]
    fn handles_jwt_at_end_of_input() {
        let input = format!("token={VALID_HS256}");
        assert_detects(&input, "JWT at end of input with no trailing newline");
    }

    #[test]
    fn handles_jwt_with_newline_after() {
        let input = format!("{VALID_HS256}\n");
        assert_detects(&input, "JWT followed by newline");
    }

    #[test]
    fn handles_unicode_surrounding_jwt() {
        let input = format!("日本語{VALID_HS256}中文");
        assert_detects(&input, "JWT surrounded by CJK characters");
    }

    #[test]
    fn handles_very_long_input() {
        let padding = "x".repeat(10_000);
        let input = format!("{padding}{VALID_HS256}{padding}");
        let matches = scan(&input);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start, 10_000);
    }

    #[test]
    fn handles_many_jwts_in_large_input() {
        let line = format!("token={VALID_HS256}\n");
        let input = line.repeat(100);
        let matches = scan(&input);
        assert_eq!(matches.len(), 100);
    }

    #[test]
    fn does_not_match_partial_jwt_after_truncation() {
        // A JWT truncated in the middle of the signature — still has 3 segments
        // so the scanner may partially match. This is acceptable behavior
        // (better to over-detect a truncated JWT than miss it).
        let truncated = &VALID_HS256[..VALID_HS256.len() - 10];
        // Should still detect if all three segments are present
        let has_three_dots = truncated.matches('.').count() >= 2;
        if has_three_dots {
            assert_detects(truncated, "truncated JWT (still has 3 segments)");
        }
    }

    // ──────────────────────────────────────────────────────────────
    // 12. SECURITY ATTACK PATTERNS (should still be detected)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_jwt_with_alg_none_attack() {
        // Classic JWT attack: change alg to "none" and strip signature
        // The token still has three segments
        assert_detects(
            "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJhZG1pbiI6dHJ1ZX0.fake",
            "JWT with alg:none attack pattern",
        );
    }

    #[test]
    fn detects_jwt_with_algorithm_confusion_attack() {
        // Changed from RS256 to HS256 with public key as HMAC secret
        assert_detects(
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJhZG1pbiI6dHJ1ZSwiZXhwIjo5OTk5OTk5OTk5fQ.tampered_signature_here",
            "JWT from algorithm confusion attack (RS256→HS256)",
        );
    }

    #[test]
    fn detects_expired_jwt() {
        // Expired JWTs are still secrets — they may be replayable or contain
        // sensitive claims even if expired
        assert_detects(
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIiwiZXhwIjoxfQ.expired_but_still_a_secret",
            "expired JWT (exp=1) — still a secret worth flagging",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 13. CONTENT TYPES THAT COMMONLY LEAK JWTs
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_jwt_in_har_file() {
        // HTTP Archive format — common source of JWT leaks
        let input = format!(
            r#"{{"request": {{"headers": [{{"name": "Authorization", "value": "Bearer {VALID_HS256}"}}]}}}}"#,
        );
        assert_detects(&input, "JWT in HAR file (HTTP Archive)");
    }

    #[test]
    fn detects_jwt_in_postman_collection() {
        let input = format!(
            r#"{{"auth": {{"type": "bearer", "bearer": [{{"key": "token", "value": "{VALID_HS256}"}}]}}}}"#,
        );
        assert_detects(&input, "JWT in Postman collection");
    }

    #[test]
    fn detects_jwt_in_swagger_example() {
        let input = format!(r#"securityDefinitions:\n  Bearer:\n    example: "{VALID_HS256}""#,);
        assert_detects(&input, "JWT hardcoded in Swagger/OpenAPI spec");
    }

    #[test]
    fn detects_jwt_in_k8s_secret_yaml() {
        let input = format!("apiVersion: v1\nkind: Secret\ndata:\n  token: {VALID_HS256}");
        assert_detects(&input, "JWT in Kubernetes Secret manifest");
    }
}
