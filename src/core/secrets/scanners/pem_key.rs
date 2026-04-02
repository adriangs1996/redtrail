use regex::Regex;

use crate::core::secrets::engine::{SecretMatch, SecretScanner};

struct PemKeyScanner;
impl SecretScanner for PemKeyScanner {
    fn scan(&self, input: &str) -> Vec<SecretMatch> {
        let re = Regex::new(
            r"-----BEGIN (?:RSA |EC |OPENSSH |DSA |ENCRYPTED )?PRIVATE KEY-----[\s\S]*?-----END (?:RSA |EC |OPENSSH |DSA |ENCRYPTED )?PRIVATE KEY-----",
        )
        .unwrap();
        re.find_iter(input)
            .map(|m| SecretMatch {
                start: m.start(),
                end: m.end(),
                label: "private_key".into(),
            })
            .collect()
    }
}
inventory::submit!(&PemKeyScanner as &dyn SecretScanner);

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(input: &str) -> Vec<SecretMatch> {
        PemKeyScanner.scan(input)
    }

    fn assert_detects(input: &str, msg: &str) {
        let matches = scan(input);
        assert!(!matches.is_empty(), "should detect: {msg}");
    }

    fn assert_ignores(input: &str, msg: &str) {
        let matches = scan(input);
        assert!(matches.is_empty(), "should NOT detect: {msg}");
    }

    // ──────────────────────────────────────────────────────────────
    // 1. KEY TYPE VARIANTS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_rsa_private_key_header() {
        assert_detects(
            "-----BEGIN RSA PRIVATE KEY-----\nMIIE...data...\n-----END RSA PRIVATE KEY-----",
            "RSA private key PEM block",
        );
    }

    #[test]
    fn detects_generic_private_key() {
        assert_detects(
            "-----BEGIN PRIVATE KEY-----\nMIIE...data...\n-----END PRIVATE KEY-----",
            "generic PKCS#8 private key",
        );
    }

    #[test]
    fn detects_ec_private_key() {
        assert_detects(
            "-----BEGIN EC PRIVATE KEY-----\nMHQC...data...\n-----END EC PRIVATE KEY-----",
            "EC private key",
        );
    }

    #[test]
    fn detects_openssh_private_key() {
        assert_detects(
            "-----BEGIN OPENSSH PRIVATE KEY-----\nb3Blb...data...\n-----END OPENSSH PRIVATE KEY-----",
            "OpenSSH private key",
        );
    }

    #[test]
    fn detects_dsa_private_key() {
        assert_detects(
            "-----BEGIN DSA PRIVATE KEY-----\nMIIB...data...\n-----END DSA PRIVATE KEY-----",
            "DSA private key",
        );
    }

    #[test]
    fn detects_encrypted_private_key() {
        assert_detects(
            "-----BEGIN ENCRYPTED PRIVATE KEY-----\nMIIE...data...\n-----END ENCRYPTED PRIVATE KEY-----",
            "encrypted PKCS#8 private key",
        );
    }

    // ──────────────────────────────────────────────────────────────
    // 2. REAL-WORLD EMBEDDING CONTEXTS
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_key_in_env_var() {
        let input = r#"PRIVATE_KEY="-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEA0Z3VS5JJ...
-----END RSA PRIVATE KEY-----""#;
        assert_detects(input, "PEM key in env var assignment");
    }

    #[test]
    fn detects_key_in_json() {
        let input =
            r#"{"private_key": "-----BEGIN PRIVATE KEY-----\nMIIE...\n-----END PRIVATE KEY-----"}"#;
        assert_detects(input, "PEM key in JSON value");
    }

    #[test]
    fn detects_key_in_cat_output() {
        let input = "$ cat ~/.ssh/id_rsa\n-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAA...\n-----END OPENSSH PRIVATE KEY-----\n$";
        assert_detects(input, "PEM key in cat command output");
    }

    // ──────────────────────────────────────────────────────────────
    // 3. MATCH METADATA
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn match_covers_entire_pem_block() {
        let input = "before\n-----BEGIN PRIVATE KEY-----\ndata\n-----END PRIVATE KEY-----\nafter";
        let matches = scan(input);
        assert_eq!(matches.len(), 1);
        let matched = &input[matches[0].start..matches[0].end];
        assert!(matched.starts_with("-----BEGIN"));
        assert!(matched.ends_with("-----END PRIVATE KEY-----"));
    }

    #[test]
    fn label_is_private_key() {
        let input = "-----BEGIN PRIVATE KEY-----\ndata\n-----END PRIVATE KEY-----";
        let matches = scan(input);
        assert_eq!(matches[0].label, "private_key");
    }

    #[test]
    fn detects_multiple_keys_in_same_input() {
        let input = "-----BEGIN RSA PRIVATE KEY-----\ndata1\n-----END RSA PRIVATE KEY-----\n\n-----BEGIN EC PRIVATE KEY-----\ndata2\n-----END EC PRIVATE KEY-----";
        let matches = scan(input);
        assert_eq!(matches.len(), 2);
    }

    // ──────────────────────────────────────────────────────────────
    // 4. TRUE NEGATIVES
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn ignores_public_key() {
        assert_ignores(
            "-----BEGIN PUBLIC KEY-----\ndata\n-----END PUBLIC KEY-----",
            "public key — not secret",
        );
    }

    #[test]
    fn ignores_certificate() {
        assert_ignores(
            "-----BEGIN CERTIFICATE-----\ndata\n-----END CERTIFICATE-----",
            "certificate — not a private key",
        );
    }

    #[test]
    fn ignores_begin_without_end() {
        assert_ignores(
            "-----BEGIN RSA PRIVATE KEY-----\ntruncated data here",
            "incomplete PEM block — no END marker",
        );
    }

    #[test]
    fn ignores_plain_text() {
        assert_ignores("just some normal text", "no PEM content");
    }

    #[test]
    fn ignores_empty_input() {
        assert_ignores("", "empty input");
    }
}
