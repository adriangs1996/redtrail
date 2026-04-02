use regex::Regex;

use crate::core::secrets::engine::{SecretMatch, SecretScanner};

struct GithubTokenScanner;
impl SecretScanner for GithubTokenScanner {
    fn scan(&self, input: &str) -> Vec<SecretMatch> {
        let mut matches = Vec::new();

        let prefixed_re = Regex::new(r"gh[pousr]_[A-Za-z0-9]{36,}").unwrap();
        for m in prefixed_re.find_iter(input) {
            if m.start() > 0 && input.as_bytes()[m.start() - 1].is_ascii_alphanumeric() {
                let run = input[..m.start()]
                    .bytes()
                    .rev()
                    .take_while(|b| b.is_ascii_alphanumeric())
                    .count();
                if run <= 5 {
                    continue;
                }
            }
            matches.push(SecretMatch {
                start: m.start(),
                end: m.end(),
                label: "github_token".into(),
            });
        }

        let legacy_re = Regex::new(r"[0-9a-f]{40}").unwrap();
        for m in legacy_re.find_iter(input) {
            let prev = m
                .start()
                .checked_sub(1)
                .and_then(|i| input.as_bytes().get(i).copied());
            let left_ok = prev
                .is_none_or(|b| !b.is_ascii_hexdigit() && b != b'_' && !b.is_ascii_alphabetic());
            let right_ok = input
                .as_bytes()
                .get(m.end())
                .is_none_or(|b| !b.is_ascii_hexdigit());
            if !left_ok || !right_ok {
                continue;
            }
            // Skip if overlapping with a prefixed token match
            if matches
                .iter()
                .any(|e| e.start < m.end() && m.start() < e.end)
            {
                continue;
            }
            matches.push(SecretMatch {
                start: m.start(),
                end: m.end(),
                label: "github_token".into(),
            });
        }

        matches
    }
}
inventory::submit!(&GithubTokenScanner as &dyn SecretScanner);

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(input: &str) -> Vec<SecretMatch> {
        GithubTokenScanner.scan(input)
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
    // 1. PERSONAL ACCESS TOKENS (ghp_ — classic & fine-grained)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_classic_pat_ghp() {
        assert_detects(
            "ghp_1234567890abcdef1234567890abcdef12345678",
            "classic personal access token (ghp_)",
        );
    }

    #[test]
    fn detects_fine_grained_pat_ghp() {
        // Fine-grained PATs also use ghp_ but are longer (typically 82+ chars)
        assert_detects(
            "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUV",
            "fine-grained personal access token (longer ghp_)",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 2. OAUTH ACCESS TOKENS (gho_)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_oauth_token_gho() {
        assert_detects(
            "gho_abcdefghijklmnopqrstuvwxyz1234567890",
            "OAuth access token (gho_)",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 3. USER-TO-SERVER TOKENS (ghu_ — GitHub App)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_user_to_server_token_ghu() {
        assert_detects(
            "ghu_ABCDEFGHIJ1234567890abcdefghij123456",
            "user-to-server token (ghu_)",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 4. SERVER-TO-SERVER TOKENS (ghs_ — GitHub App installation)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_server_to_server_token_ghs() {
        assert_detects(
            "ghs_1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ",
            "server-to-server installation token (ghs_)",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 5. REFRESH TOKENS (ghr_)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_refresh_token_ghr() {
        assert_detects(
            "ghr_abcdef1234567890ABCDEF1234567890abcdef",
            "refresh token (ghr_)",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 6. GITHUB CLASSIC TOKENS (ghp_ with 40 hex chars — v1 format)
    //    Pre-2021 tokens: 40 hex characters after `ghp_`
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_classic_v1_token_exact_36_chars() {
        assert_detects(
            "ghp_aB3dE5fG7hI9jK1lM3nO5pQ7rS9tU1vW3xYz",
            "classic token with exactly 36 chars after prefix",
        );
    }

    #[test]
    fn detects_token_with_40_chars_after_prefix() {
        assert_detects(
            "ghp_1234567890abcdef1234567890abcdef12345678",
            "token with 40 chars after prefix",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 7. LEGACY / PRE-PREFIX TOKENS (before GitHub added prefixes)
    //    Old-style tokens were 40 hex chars with no prefix.
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_legacy_40_hex_token_no_prefix() {
        // Before 2021, GitHub PATs were just 40 hex chars.
        // These are still valid and in circulation.
        assert_detects(
            "0123456789abcdef0123456789abcdef01234567",
            "legacy GitHub token (40 hex chars, no prefix)",
        );
    }

    #[test]
    fn detects_legacy_token_in_authorization_header() {
        assert_detects(
            "Authorization: token 0123456789abcdef0123456789abcdef01234567",
            "legacy token in Authorization header",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 8. REAL-WORLD EMBEDDING CONTEXTS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_token_in_env_var_export() {
        assert_detects(
            "export GITHUB_TOKEN=ghp_1234567890abcdef1234567890abcdef12345678",
            "token in shell export",
        );
    }

    #[test]
    fn detects_token_in_env_var_inline() {
        assert_detects(
            "GITHUB_TOKEN=ghp_1234567890abcdef1234567890abcdef12345678 ./deploy.sh",
            "token in inline env var",
        );
    }

    #[test]
    fn detects_token_in_dotenv_file() {
        assert_detects(
            "GITHUB_TOKEN=ghp_1234567890abcdef1234567890abcdef12345678",
            "token in .env file",
        );
    }

    #[test]
    fn detects_token_in_git_remote_url() {
        assert_detects(
            "https://ghp_1234567890abcdef1234567890abcdef12345678@github.com/owner/repo.git",
            "token embedded in git remote URL",
        );
    }

    #[test]
    fn detects_token_in_curl_header() {
        assert_detects(
            r#"curl -H "Authorization: Bearer ghp_1234567890abcdef1234567890abcdef12345678" https://api.github.com/user"#,
            "token in curl Authorization header",
        );
    }

    #[test]
    fn detects_token_in_curl_token_header() {
        assert_detects(
            r#"curl -H "Authorization: token ghp_1234567890abcdef1234567890abcdef12345678" https://api.github.com/repos"#,
            "token in curl token-style Authorization header",
        );
    }

    #[test]
    fn detects_token_in_json_config() {
        assert_detects(
            r#"{"github_token": "ghp_1234567890abcdef1234567890abcdef12345678"}"#,
            "token in JSON config",
        );
    }

    #[test]
    fn detects_token_in_yaml_config() {
        assert_detects(
            "github_token: ghp_1234567890abcdef1234567890abcdef12345678",
            "token in YAML config",
        );
    }

    #[test]
    fn detects_token_in_github_actions_secret_ref() {
        // Someone accidentally hardcoded instead of using ${{ secrets.TOKEN }}
        assert_detects(
            "  env:\n    GITHUB_TOKEN: ghp_1234567890abcdef1234567890abcdef12345678",
            "hardcoded token in GitHub Actions workflow",
        );
    }

    #[test]
    fn detects_token_in_npmrc() {
        assert_detects(
            "//npm.pkg.github.com/:_authToken=ghp_1234567890abcdef1234567890abcdef12345678",
            "token in .npmrc for GitHub Packages",
        );
    }

    #[test]
    fn detects_token_in_docker_build_arg() {
        assert_detects(
            "docker build --build-arg GITHUB_TOKEN=ghp_1234567890abcdef1234567890abcdef12345678 .",
            "token in docker build arg",
        );
    }

    #[test]
    fn detects_token_in_terraform_provider() {
        assert_detects(
            r#"token = "ghp_1234567890abcdef1234567890abcdef12345678""#,
            "token in Terraform GitHub provider config",
        );
    }

    #[test]
    fn detects_token_in_python_source() {
        assert_detects(
            r#"g = Github("ghp_1234567890abcdef1234567890abcdef12345678")"#,
            "token in Python PyGithub constructor",
        );
    }

    #[test]
    fn detects_token_in_javascript_source() {
        assert_detects(
            r#"const octokit = new Octokit({ auth: "ghp_1234567890abcdef1234567890abcdef12345678" });"#,
            "token in JS Octokit constructor",
        );
    }

    #[test]
    fn detects_token_in_gitconfig() {
        assert_detects(
            "[url \"https://ghp_1234567890abcdef1234567890abcdef12345678@github.com/\"]",
            "token in .gitconfig URL rewrite",
        );
    }

    #[test]
    fn detects_token_in_ci_log_output() {
        assert_detects(
            "2024-01-15 10:30:00 [INFO] Using token ghp_1234567890abcdef1234567890abcdef12345678 for auth",
            "token leaked in CI log output",
        );
    }

    #[test]
    fn detects_token_in_csv_export() {
        assert_detects(
            "user,ghp_1234567890abcdef1234567890abcdef12345678,admin,true",
            "token in CSV export",
        );
    }

    #[test]
    fn detects_token_in_xml() {
        assert_detects(
            "<token>ghp_1234567890abcdef1234567890abcdef12345678</token>",
            "token in XML element",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 9. MULTIPLE TOKENS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_multiple_tokens_in_same_input() {
        let input = "old=ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAaaaa new=ghp_BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBbbbb";
        let matches = scan(input);
        assert_eq!(matches.len(), 2, "should find two distinct tokens");
    }

    #[test]
    fn detects_different_prefix_types_in_same_input() {
        let input = "pat=ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA install=ghs_BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
        let matches = scan(input);
        assert_eq!(matches.len(), 2, "should find both ghp_ and ghs_ tokens");
    }

    #[test]
    fn detects_tokens_on_separate_lines() {
        let input = "GITHUB_TOKEN=ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\nGH_INSTALL=ghs_BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB\n";
        let matches = scan(input);
        assert_eq!(matches.len(), 2, "should find tokens across lines");
    }

    // ──────────────────────────────────────────────────────────────
    // 10. MATCH METADATA (position, label)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn match_start_and_end_correct_at_offset() {
        let input = "token=ghp_1234567890abcdef1234567890abcdef12345678";
        let matches = scan(input);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start, 6); // after "token="
    }

    #[test]
    fn match_start_correct_at_beginning() {
        let input = "ghp_1234567890abcdef1234567890abcdef12345678 rest";
        let matches = scan(input);
        assert_eq!(matches[0].start, 0);
    }

    #[test]
    fn match_label_is_github_token() {
        let matches = scan("ghp_1234567890abcdef1234567890abcdef12345678");
        assert_eq!(matches[0].label, "github_token");
    }

    #[test]
    fn match_label_same_for_all_prefix_types() {
        for prefix in &["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
            let token = format!("{prefix}{}", "A".repeat(36));
            let matches = scan(&token);
            assert_eq!(
                matches[0].label, "github_token",
                "label should be github_token for {prefix} tokens",
            );
        }
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
            "plain English",
        );
    }

    #[test]
    fn ignores_env_var_reference_no_value() {
        assert_ignores(
            "echo $GITHUB_TOKEN",
            "shell variable reference without value",
        );
    }

    #[test]
    fn ignores_secrets_template_reference() {
        assert_ignores(
            "${{ secrets.GITHUB_TOKEN }}",
            "GitHub Actions secrets template",
        );
    }

    #[test]
    fn ignores_ghp_prefix_too_short() {
        // Only 35 chars after prefix — under the 36 minimum
        assert_ignores(
            "ghp_12345678901234567890123456789012345",
            "ghp_ with only 35 chars (too short)",
        );
    }

    #[test]
    fn ignores_gh_without_type_char() {
        assert_ignores(
            "gh_1234567890abcdef1234567890abcdef12345678",
            "gh_ prefix — missing type character",
        );
    }

    #[test]
    fn ignores_ghx_invalid_type_char() {
        assert_ignores(
            "ghx_1234567890abcdef1234567890abcdef12345678",
            "ghx_ prefix — x is not a valid token type",
        );
    }

    #[test]
    fn ignores_ghp_without_underscore() {
        assert_ignores(
            "ghp1234567890abcdef1234567890abcdef12345678",
            "ghp without underscore separator",
        );
    }

    #[test]
    #[allow(non_snake_case)]
    fn ignores_uppercase_prefix_GHP() {
        assert_ignores(
            "GHP_1234567890abcdef1234567890abcdef12345678",
            "GHP_ uppercase prefix — GitHub tokens are lowercase prefix",
        );
    }

    #[test]
    fn ignores_github_app_jwt() {
        // GitHub App JWTs are separate from token prefixes
        assert_ignores(
            "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpYXQiOi0zMH0.signature",
            "GitHub App JWT — handled by JWT scanner, not token scanner",
        );
    }

    #[test]
    fn ignores_github_webhook_secret() {
        // Webhook secrets are random strings, not prefixed tokens
        assert_ignores(
            "webhook_secret=mysecretvalue12345",
            "GitHub webhook secret — not a prefixed token",
        );
    }

    #[test]
    fn ignores_random_string_starting_with_gh() {
        assert_ignores(
            "ghosts_in_the_machine_are_not_tokens!",
            "word starting with 'gh'",
        );
    }

    #[test]
    fn ignores_ghp_with_special_chars_in_body() {
        // Tokens are alphanumeric only; special chars break the match
        assert_ignores(
            "ghp_1234!@#$5678abcdef1234567890abcdef1234",
            "ghp_ with special chars embedded — not a valid token",
        );
    }

    #[test]
    fn ignores_uuid() {
        assert_ignores(
            "550e8400-e29b-41d4-a716-446655440000",
            "UUID — not a GitHub token",
        );
    }

    #[test]
    fn ignores_sha256_hash() {
        assert_ignores(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "SHA-256 hash — not a GitHub token",
        );
    }

    #[test]
    fn ignores_npm_token() {
        assert_ignores(
            "npm_1234567890abcdef1234567890abcdef12345678",
            "npm token — not a GitHub token",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 12. BOUNDARY / EDGE CASES
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn handles_token_adjacent_to_special_chars() {
        assert_detects(
            "(ghp_1234567890abcdef1234567890abcdef12345678)",
            "token in parentheses",
        );
        assert_detects(
            "\"ghp_1234567890abcdef1234567890abcdef12345678\"",
            "token in double quotes",
        );
        assert_detects(
            "'ghp_1234567890abcdef1234567890abcdef12345678'",
            "token in single quotes",
        );
    }

    #[test]
    fn handles_token_after_equals_no_space() {
        assert_detects(
            "TOKEN=ghp_1234567890abcdef1234567890abcdef12345678",
            "token after = with no space",
        );
    }

    #[test]
    fn handles_token_after_colon_space() {
        assert_detects(
            "token: ghp_1234567890abcdef1234567890abcdef12345678",
            "token after colon-space (YAML)",
        );
    }

    #[test]
    fn handles_token_at_end_of_input() {
        assert_detects(
            "val=ghp_1234567890abcdef1234567890abcdef12345678",
            "token at end of input with no trailing newline",
        );
    }

    #[test]
    fn handles_token_with_newline_immediately_after() {
        assert_detects(
            "ghp_1234567890abcdef1234567890abcdef12345678\n",
            "token followed by newline",
        );
    }

    #[test]
    fn does_not_over_match_into_trailing_alphanum() {
        // If extra alphanumeric chars follow, the regex is greedy ({36,})
        // so it will consume them — but the whole match should still be flagged.
        let input = "ghp_1234567890abcdef1234567890abcdef12345678EXTRA";
        let matches = scan(input);
        assert!(!matches.is_empty(), "should detect the token");
    }

    #[test]
    fn handles_unicode_surrounding_token() {
        assert_detects(
            "日本語ghp_1234567890abcdef1234567890abcdef12345678中文",
            "token surrounded by CJK characters",
        );
    }

    #[test]
    fn handles_very_long_input() {
        let padding = "x".repeat(10_000);
        let input = format!("{padding}ghp_1234567890abcdef1234567890abcdef12345678{padding}");
        let matches = scan(&input);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start, 10_000);
    }

    #[test]
    fn handles_many_tokens_in_large_input() {
        let line = "token=ghp_1234567890abcdef1234567890abcdef12345678\n";
        let input = line.repeat(100);
        let matches = scan(&input);
        assert_eq!(matches.len(), 100);
    }

    #[test]
    fn should_not_match_prefix_embedded_in_longer_word() {
        // "xghp_..." — prefix not at a word boundary
        assert_ignores(
            "xghp_1234567890abcdef1234567890abcdef12345678",
            "ghp_ embedded after leading char — not at boundary",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 13. GITHUB ENTERPRISE / SPECIAL FORMATS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_token_with_github_enterprise_url() {
        assert_detects(
            "https://ghp_1234567890abcdef1234567890abcdef12345678@github.example.com/org/repo.git",
            "token in GitHub Enterprise remote URL",
        );
    }

    #[test]
    fn detects_token_in_gh_cli_config() {
        let input =
            "hosts:\n  github.com:\n    oauth_token: ghp_1234567890abcdef1234567890abcdef12345678";
        assert_detects(input, "token in gh CLI config (~/.config/gh/hosts.yml)");
    }

    // ──────────────────────────────────────────────────────────────
    // 14. ALL PREFIX TYPES — systematic coverage
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_all_five_prefix_types() {
        let suffixes = "A".repeat(36);
        for (prefix, desc) in &[
            ("ghp_", "personal access token"),
            ("gho_", "OAuth access token"),
            ("ghu_", "user-to-server token"),
            ("ghs_", "server-to-server token"),
            ("ghr_", "refresh token"),
        ] {
            let token = format!("{prefix}{suffixes}");
            assert_detects(&token, desc);
        }
    }

    #[test]
    fn ignores_all_invalid_single_char_prefixes() {
        let suffixes = "A".repeat(36);
        for c in b'a'..=b'z' {
            let ch = c as char;
            if "pousr".contains(ch) {
                continue; // valid prefix types
            }
            let token = format!("gh{ch}_{suffixes}");
            assert_ignores(&token, &format!("gh{ch}_ — invalid type char"));
        }
    }
}
