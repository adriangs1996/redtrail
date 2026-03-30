/// Escape a string for safe inclusion in markdown.
pub fn escape(s: &str) -> String {
    s.replace('|', "\\|")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
}
