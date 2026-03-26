use regex::Regex;

use crate::core::secrets::engine::{SecretMatch, SecretScanner};

struct JwtScanner;
impl SecretScanner for JwtScanner {
    fn scan(&self, input: &str) -> Vec<SecretMatch> {
        let re = Regex::new(r"eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+").unwrap();
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
