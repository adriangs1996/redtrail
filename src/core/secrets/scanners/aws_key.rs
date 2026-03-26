use regex::Regex;

use crate::core::secrets::engine::{SecretMatch, SecretScanner};

struct AwsKeyScanner;
impl SecretScanner for AwsKeyScanner {
    fn scan(&self, input: &str) -> Vec<SecretMatch> {
        let re = Regex::new(r"A[KS]IA[A-Z0-9]{16}").unwrap();
        re.find_iter(input)
            .map(|m| SecretMatch {
                start: m.start(),
                end: m.end(),
                label: "aws_key".into(),
            })
            .collect()
    }
}
inventory::submit!(&AwsKeyScanner as &dyn SecretScanner);
