const SQL_KEYWORDS: &[&str] = &[
    "select", "from", "where", "join", "inner", "outer", "left", "right",
    "on", "and", "or", "not", "in", "like", "between", "is", "null",
    "order", "by", "group", "having", "limit", "offset", "as", "distinct",
    "insert", "update", "delete", "create", "drop", "alter", "table",
    "index", "pragma", "explain", "union", "all", "exists", "case",
    "when", "then", "else", "end", "asc", "desc", "count", "sum",
    "avg", "min", "max", "values", "set", "into",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    Identifier,
    StringLiteral,
    Number,
    Operator,
    Whitespace,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub text: String,
    pub kind: TokenKind,
}

pub fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        if ch.is_whitespace() {
            while i < chars.len() && chars[i].is_whitespace() { i += 1; }
            continue;
        }

        if ch == '\'' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '\'' { i += 1; }
            if i < chars.len() { i += 1; }
            tokens.push(Token { text: chars[start..i].iter().collect(), kind: TokenKind::StringLiteral });
            continue;
        }

        if ch.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') { i += 1; }
            tokens.push(Token { text: chars[start..i].iter().collect(), kind: TokenKind::Number });
            continue;
        }

        if ch.is_alphabetic() || ch == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') { i += 1; }
            let word: String = chars[start..i].iter().collect();
            let kind = if SQL_KEYWORDS.contains(&word.to_lowercase().as_str()) {
                TokenKind::Keyword
            } else {
                TokenKind::Identifier
            };
            tokens.push(Token { text: word, kind });
            continue;
        }

        tokens.push(Token { text: ch.to_string(), kind: TokenKind::Operator });
        i += 1;
    }

    tokens
}
