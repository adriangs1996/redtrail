use regex::Regex;

use crate::core::secrets::engine::{SecretMatch, SecretScanner};

struct ConnectionStringScanner;
impl SecretScanner for ConnectionStringScanner {
    fn scan(&self, input: &str) -> Vec<SecretMatch> {
        // Match protocol://user:password@host patterns for known DB protocols
        let re = Regex::new(
            r"(?:postgresql|postgres|mysql|mongodb(?:\+srv)?|redis|amqp)://[^:/@\s]+:([^@\s]+)@",
        )
        .unwrap();
        re.captures_iter(input)
            .map(|cap| {
                let password = cap.get(1).unwrap();
                SecretMatch {
                    start: password.start(),
                    end: password.end(),
                    label: "connection_password".into(),
                }
            })
            .collect()
    }
}
inventory::submit!(&ConnectionStringScanner as &dyn SecretScanner);

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(input: &str) -> Vec<SecretMatch> {
        ConnectionStringScanner.scan(input)
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
    // 1. PROTOCOL VARIANTS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_postgres_connection_string() {
        assert_detects(
            "postgresql://admin:s3cret@db.example.com:5432/mydb",
            "PostgreSQL connection string with password",
        );
    }

    #[test]
    fn detects_mysql_connection_string() {
        assert_detects(
            "mysql://root:hunter2@localhost:3306/app",
            "MySQL connection string with password",
        );
    }

    #[test]
    fn detects_mongodb_connection_string() {
        assert_detects(
            "mongodb://user:p%40ssw0rd@mongo.example.com:27017/admin",
            "MongoDB connection string with URL-encoded password",
        );
    }

    #[test]
    fn detects_redis_connection_string() {
        assert_detects(
            "redis://default:mypassword@redis.example.com:6379/0",
            "Redis connection string with password",
        );
    }

    #[test]
    fn detects_amqp_connection_string() {
        assert_detects(
            "amqp://guest:guest@rabbitmq.local:5672/",
            "AMQP connection string with password",
        );
    }

    #[test]
    fn detects_mongodb_srv_connection_string() {
        assert_detects(
            "mongodb+srv://user:pass@cluster0.abc.mongodb.net/db",
            "MongoDB SRV connection string",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 2. REDACTS ONLY THE PASSWORD PORTION
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn match_covers_only_password() {
        let input = "postgresql://admin:s3cret@db.example.com/mydb";
        let matches = scan(input);
        assert_eq!(matches.len(), 1);
        assert_eq!(&input[matches[0].start..matches[0].end], "s3cret");
    }

    #[test]
    fn label_is_connection_password() {
        let input = "mysql://root:hunter2@localhost/db";
        let matches = scan(input);
        assert_eq!(matches[0].label, "connection_password");
    }

    // ──────────────────────────────────────────────────────────────
    // 3. REAL-WORLD CONTEXTS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_in_env_var_assignment() {
        assert_detects(
            "DATABASE_URL=postgresql://admin:s3cret@db.example.com:5432/mydb",
            "connection string in env var",
        );
    }

    #[test]
    fn detects_in_docker_compose() {
        assert_detects(
            "      - MONGO_URL=mongodb://user:pass@mongo:27017/app",
            "connection string in docker-compose",
        );
    }

    #[test]
    fn detects_password_with_special_chars() {
        assert_detects(
            "postgresql://user:p%40ss%3Aw0rd@host/db",
            "URL-encoded special chars in password",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 4. TRUE NEGATIVES
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn ignores_url_without_credentials() {
        assert_ignores(
            "postgresql://db.example.com:5432/mydb",
            "connection string without user:pass",
        );
    }

    #[test]
    fn ignores_http_url_with_auth() {
        // HTTP URLs are not database connection strings
        assert_ignores(
            "http://user:pass@example.com",
            "HTTP URL — not a DB connection string",
        );
    }

    #[test]
    fn ignores_url_with_user_but_no_password() {
        assert_ignores(
            "postgresql://admin@db.example.com/mydb",
            "connection with user but no password",
        );
    }

    #[test]
    fn ignores_plain_text() {
        assert_ignores("just some normal text", "no connection string");
    }

    #[test]
    fn ignores_empty_input() {
        assert_ignores("", "empty input");
    }
}
