use redtrail::workflows::query::highlight::{tokenize, TokenKind};

#[test]
fn tokenizes_keywords() {
    let tokens = tokenize("SELECT name FROM hosts WHERE id = 1");
    assert_eq!(tokens[0].kind, TokenKind::Keyword);
    assert_eq!(tokens[2].kind, TokenKind::Keyword);
    assert_eq!(tokens[4].kind, TokenKind::Keyword);
}

#[test]
fn tokenizes_strings() {
    let tokens = tokenize("SELECT * FROM hosts WHERE name = 'target'");
    let string_token = tokens.iter().find(|t| t.kind == TokenKind::StringLiteral);
    assert!(string_token.is_some());
    assert_eq!(string_token.unwrap().text, "'target'");
}

#[test]
fn tokenizes_numbers() {
    let tokens = tokenize("SELECT * FROM ports WHERE port = 8080");
    let num = tokens.iter().find(|t| t.kind == TokenKind::Number);
    assert!(num.is_some());
}

#[test]
fn tokenizes_identifiers() {
    let tokens = tokenize("SELECT name FROM hosts");
    assert_eq!(tokens[1].kind, TokenKind::Identifier);
    assert_eq!(tokens[3].kind, TokenKind::Identifier);
}
