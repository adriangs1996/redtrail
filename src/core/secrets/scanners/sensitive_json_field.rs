use regex::Regex;

use super::classify_key;
use crate::core::secrets::engine::{SecretMatch, SecretScanner};

struct SensitiveJsonFieldScanner;
impl SecretScanner for SensitiveJsonFieldScanner {
    fn scan(&self, input: &str) -> Vec<SecretMatch> {
        let re =
            Regex::new(r#"(?i)"([^"]*(?:SECRET|PASSWORD|PASSWD)[^"]*)"\s*:\s*"([^"]+)""#).unwrap();
        re.captures_iter(input)
            .map(|cap| {
                let val = cap.get(2).unwrap();
                let key = cap.get(1).unwrap().as_str();
                SecretMatch {
                    start: val.start(),
                    end: val.end(),
                    label: classify_key(key),
                }
            })
            .collect()
    }
    fn priority(&self) -> u8 {
        10
    }
}
inventory::submit!(&SensitiveJsonFieldScanner as &dyn SecretScanner);
