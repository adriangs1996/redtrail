use regex::Regex;

use crate::core::secrets::engine::{SecretMatch, SecretScanner};

struct CliPasswordFlagScanner;
impl SecretScanner for CliPasswordFlagScanner {
    fn scan(&self, input: &str) -> Vec<SecretMatch> {
        let mut matches = Vec::new();

        // mysql/mysqldump -pPASSWORD (no space, unquoted)
        let unquoted =
            Regex::new(r#"(?:mysql|mysqldump)\b[^\n]*\s-p([^\s'"]\S*)(?:\s|$)"#).unwrap();
        for cap in unquoted.captures_iter(input) {
            let val = cap.get(1).unwrap();
            matches.push(SecretMatch {
                start: val.start(),
                end: val.end(),
                label: "password".into(),
            });
        }

        // mysql/mysqldump -p'PASSWORD' (single-quoted)
        let single_q = Regex::new(r"(?:mysql|mysqldump)\b[^\n]*\s-p'([^']+)'").unwrap();
        for cap in single_q.captures_iter(input) {
            let val = cap.get(1).unwrap();
            matches.push(SecretMatch {
                start: val.start(),
                end: val.end(),
                label: "password".into(),
            });
        }

        // mysql/mysqldump -p"PASSWORD" (double-quoted)
        let double_q = Regex::new(r#"(?:mysql|mysqldump)\b[^\n]*\s-p"([^"]+)""#).unwrap();
        for cap in double_q.captures_iter(input) {
            let val = cap.get(1).unwrap();
            matches.push(SecretMatch {
                start: val.start(),
                end: val.end(),
                label: "password".into(),
            });
        }

        // mysql/mysqldump --password=VALUE
        let long_flag = Regex::new(r"(?:mysql|mysqldump)\b[^\n]*\s--password=(\S+)").unwrap();
        for cap in long_flag.captures_iter(input) {
            let val = cap.get(1).unwrap();
            matches.push(SecretMatch {
                start: val.start(),
                end: val.end(),
                label: "password".into(),
            });
        }

        // PGPASSWORD=value (handled by sensitive_assignment, but we catch
        // the inline-before-psql pattern specifically for context)
        let pgpass = Regex::new(r"PGPASSWORD=(\S+)").unwrap();
        for cap in pgpass.captures_iter(input) {
            let val = cap.get(1).unwrap();
            if !val.as_str().starts_with('$') {
                matches.push(SecretMatch {
                    start: val.start(),
                    end: val.end(),
                    label: "password".into(),
                });
            }
        }

        matches
    }

    fn priority(&self) -> u8 {
        5
    }
}
inventory::submit!(&CliPasswordFlagScanner as &dyn SecretScanner);

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(input: &str) -> Vec<SecretMatch> {
        CliPasswordFlagScanner.scan(input)
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
    // 1. MYSQL -p FLAG (password immediately after -p, no space)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_mysql_p_flag_no_space() {
        assert_detects(
            "mysql -u root -phunter2 mydb",
            "mysql -p with password directly attached",
        );
    }

    #[test]
    fn detects_mysql_p_flag_with_quotes() {
        assert_detects(
            "mysql -u root -p'hunter2' mydb",
            "mysql -p with single-quoted password",
        );
    }

    #[test]
    fn detects_mysql_p_flag_double_quotes() {
        assert_detects(
            r#"mysql -u root -p"hunter2" mydb"#,
            "mysql -p with double-quoted password",
        );
    }

    #[test]
    fn detects_mysqldump_p_flag() {
        assert_detects(
            "mysqldump -u admin -phunter2 --all-databases",
            "mysqldump -p with password",
        );
    }

    #[test]
    fn detects_mysql_password_long_flag() {
        assert_detects(
            "mysql --password=hunter2 -u root mydb",
            "mysql --password= long form",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 2. PSQL PASSWORD PATTERNS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_pgpassword_env_var() {
        assert_detects(
            "PGPASSWORD=hunter2 psql -U admin mydb",
            "PGPASSWORD inline env var before psql",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 3. MATCH METADATA
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn match_covers_only_password_value() {
        let input = "mysql -u root -phunter2 mydb";
        let matches = scan(input);
        assert_eq!(matches.len(), 1);
        assert_eq!(&input[matches[0].start..matches[0].end], "hunter2");
    }

    #[test]
    fn label_is_password() {
        let input = "mysql -phunter2";
        let matches = scan(input);
        assert_eq!(matches[0].label, "password");
    }

    // ──────────────────────────────────────────────────────────────
    // 4. TRUE NEGATIVES
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn ignores_mysql_p_flag_alone_no_value() {
        // -p with a space means "prompt for password" — no secret to redact
        assert_ignores(
            "mysql -u root -p mydb",
            "mysql -p with space (interactive prompt)",
        );
    }

    #[test]
    fn ignores_grep_p_flag() {
        assert_ignores(
            "grep -p pattern file.txt",
            "grep -p — not a database command",
        );
    }

    #[test]
    fn ignores_plain_text() {
        assert_ignores("just some text", "no CLI password flag");
    }

    #[test]
    fn ignores_empty_input() {
        assert_ignores("", "empty input");
    }
}
