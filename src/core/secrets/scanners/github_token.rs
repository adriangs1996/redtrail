use regex::Regex;

use crate::core::secrets::engine::{SecretMatch, SecretScanner};

struct GithubTokenScanner;
impl SecretScanner for GithubTokenScanner {
    fn scan(&self, input: &str) -> Vec<SecretMatch> {
        let re = Regex::new(r"gh[pousr]_[A-Za-z0-9]{36,}").unwrap();
        re.find_iter(input)
            .map(|m| SecretMatch {
                start: m.start(),
                end: m.end(),
                label: "github_token".into(),
            })
            .collect()
    }
}
inventory::submit!(&GithubTokenScanner as &dyn SecretScanner);
