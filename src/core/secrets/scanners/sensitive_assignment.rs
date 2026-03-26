use regex::Regex;

use super::classify_key;
use crate::core::secrets::engine::{SecretMatch, SecretScanner};

struct SensitiveAssignmentScanner;
impl SecretScanner for SensitiveAssignmentScanner {
    fn scan(&self, input: &str) -> Vec<SecretMatch> {
        let re = Regex::new(r"(?i)([A-Z_]*(?:SECRET|PASSWORD|PASSWD)[A-Z_]*)=(\S+)").unwrap();
        re.captures_iter(input)
            .filter_map(|cap| {
                let val = cap.get(2).unwrap();
                if val.as_str().starts_with('$') {
                    return None;
                }
                let key = cap.get(1).unwrap().as_str();
                Some(SecretMatch {
                    start: val.start(),
                    end: val.end(),
                    label: classify_key(key),
                })
            })
            .collect()
    }
    fn priority(&self) -> u8 {
        10
    }
}
inventory::submit!(&SensitiveAssignmentScanner as &dyn SecretScanner);
