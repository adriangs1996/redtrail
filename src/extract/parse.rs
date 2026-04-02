// Command output parsing utilities — tokenization, line splitting, field extraction by domain.

use regex::Regex;
use std::sync::OnceLock;

/// A single command segment extracted from a compound command line.
#[derive(Debug, Clone)]
pub struct CommandSegment {
    /// The trimmed command text with redirect operators stripped.
    pub raw: String,
}

/// Split a compound command line into segments on pipes, chains, and semicolons.
///
/// Respects single/double quotes and backslash escapes so that delimiters
/// inside quoted strings or after a backslash are never treated as splits.
///
/// Supported delimiters (outside quotes): `|`, `||`, `&&`, `;`
///
/// Redirect operators (`>`, `>>`, `<`, `2>`, `1>`, `2>&1`, etc.) are stripped
/// from each segment after splitting.
///
/// Empty segments (after trimming) are filtered out.
pub fn split_segments(command_raw: &str) -> Vec<CommandSegment> {
    let raw_segments = split_on_delimiters(command_raw);
    raw_segments
        .into_iter()
        .map(|s| strip_redirects(s.trim()))
        .filter(|s| !s.is_empty())
        .map(|raw| CommandSegment { raw })
        .collect()
}

// --- State machine splitter ---

/// Split the raw command string on shell delimiters, respecting quotes and escapes.
/// Returns the raw (untrimmed, unstripped) segment strings.
fn split_on_delimiters(input: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();

    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;

    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        if escaped {
            // Previous char was `\` — include this char literally regardless of what it is.
            current.push(ch);
            escaped = false;
            i += 1;
            continue;
        }

        if ch == '\\' && !in_single_quote {
            // Backslash outside single quotes starts an escape sequence.
            // Do NOT push the backslash itself — the escaped char will be
            // added on the next iteration.  This matches what a real shell
            // does when it processes \| (the | is literal, not a pipe).
            escaped = true;
            i += 1;
            continue;
        }

        if ch == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            current.push(ch);
            i += 1;
            continue;
        }

        if ch == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            current.push(ch);
            i += 1;
            continue;
        }

        // Only split on delimiters when outside all quotes.
        if !in_single_quote && !in_double_quote {
            // `&&` — and-chain
            if ch == '&' && i + 1 < len && chars[i + 1] == '&' {
                segments.push(current.clone());
                current.clear();
                i += 2; // skip both `&` chars
                continue;
            }

            // `||` — or-chain  (must be checked before lone `|`)
            if ch == '|' && i + 1 < len && chars[i + 1] == '|' {
                segments.push(current.clone());
                current.clear();
                i += 2;
                continue;
            }

            // `|` — pipe (lone)
            if ch == '|' {
                segments.push(current.clone());
                current.clear();
                i += 1;
                continue;
            }

            // `;` — sequential execution
            if ch == ';' {
                segments.push(current.clone());
                current.clear();
                i += 1;
                continue;
            }
        }

        current.push(ch);
        i += 1;
    }

    // Push the final segment (may be the only one for a simple command).
    segments.push(current);

    segments
}

// --- Redirect stripping ---

fn redirect_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Matches redirect operators and, where the target is separated by
        // whitespace, the target word as well.
        //
        // Covered patterns:
        //   2>&1              — fd-merge (no separate target word)
        //   2>/path  2> /path — stderr redirect (target may have leading space)
        //   1>/path  1> /path — explicit fd 1 output redirect
        //   >>/path  >> /path — append redirect
        //   >/path   > /path  — output redirect
        //   </path   < /path  — input redirect
        //
        // Pattern anatomy:
        //   \d*              optional leading fd (e.g. 2, 1)
        //   (?:>>|>|<)       the redirect operator (>> before > so it matches first)
        //   (?:&\d+|\s*\S+)? optional target — either &N (fd-merge) or optional
        //                    whitespace followed by a non-whitespace word
        Regex::new(
            r"\d*(?:>>|>|<)(?:&\d+|\s*\S+)?"
        ).expect("redirect regex is valid")
    })
}

/// Strip redirect operators from a segment string.
///
/// This operates on already-split, trimmed segment text so we do not need to
/// worry about quotes containing redirect characters (those segments would not
/// have been split in the first place only if the redirect appeared inside
/// quotes — but redirects inside quotes are unusual and not a real redirect).
fn strip_redirects(segment: &str) -> String {
    let result = redirect_pattern().replace_all(segment, "");
    // Collapse multiple internal spaces that may result from removal.
    result
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn split_simple() {
        let segs = split_on_delimiters("git status");
        assert_eq!(segs, vec!["git status"]);
    }

    #[test]
    fn split_pipe() {
        let segs = split_on_delimiters("a | b");
        assert_eq!(segs, vec!["a ", " b"]);
    }

    #[test]
    fn split_and_chain() {
        let segs = split_on_delimiters("a && b");
        assert_eq!(segs, vec!["a ", " b"]);
    }

    #[test]
    fn split_or_chain() {
        let segs = split_on_delimiters("a || b");
        assert_eq!(segs, vec!["a ", " b"]);
    }

    #[test]
    fn no_split_in_single_quotes() {
        let segs = split_on_delimiters("echo 'a | b'");
        assert_eq!(segs, vec!["echo 'a | b'"]);
    }

    #[test]
    fn no_split_in_double_quotes() {
        let segs = split_on_delimiters("echo \"a && b\"");
        assert_eq!(segs, vec!["echo \"a && b\""]);
    }

    #[test]
    fn escaped_pipe_not_split() {
        let segs = split_on_delimiters("echo \\| world");
        // backslash consumed, | treated as literal
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn strip_output_redirect() {
        assert_eq!(strip_redirects("cargo build > /dev/null"), "cargo build");
    }

    #[test]
    fn strip_stderr_redirect() {
        assert_eq!(strip_redirects("make 2>&1"), "make");
    }

    #[test]
    fn strip_append_redirect() {
        assert_eq!(strip_redirects("echo hi >> log.txt"), "echo hi");
    }

    #[test]
    fn strip_input_redirect() {
        assert_eq!(strip_redirects("wc -l < file.txt"), "wc -l");
    }
}
