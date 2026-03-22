use crate::config::tokenizer::{Token, TokenKind, tokenize};

// ── Helper ────────────────────────────────────────────────────────────────────

// Strip line numbers — tests only care about token kinds, not positions
fn kinds(input: &str) -> Vec<TokenKind> {
    tokenize(input).into_iter().map(|t| t.kind).collect()
}

// ── Basic tokens ──────────────────────────────────────────────────────────────

#[test]
fn test_empty_input() {
    let tokens = tokenize("");
    assert!(tokens.is_empty());
}

#[test]
fn test_single_word() {
    let tokens = tokenize("server");
    assert_eq!(tokens, vec![TokenKind::Word("server".to_string())]);
}

#[test]
fn test_braces() {
    let tokens = tokenize("{}");
    assert_eq!(tokens, vec![TokenKind::LBrace, TokenKind::RBrace]);
}

#[test]
fn test_semicolon() {
    let tokens = tokenize(";");
    assert_eq!(tokens, vec![TokenKind::Semicolon]);
}

#[test]
fn test_word_with_semicolon() {
    let tokens = tokenize("host 127.0.0.1;");
    assert_eq!(
        tokens,
        vec![
            TokenKind::Word("host".to_string()),
            TokenKind::Word("127.0.0.1".to_string()),
            TokenKind::Semicolon,
        ]
    );
}

// ── Whitespace handling ───────────────────────────────────────────────────────

#[test]
fn test_multiple_spaces_between_words() {
    let tokens = tokenize("host    127.0.0.1");
    assert_eq!(
        tokens,
        vec![
            TokenKind::Word("host".to_string()),
            TokenKind::Word("127.0.0.1".to_string()),
        ]
    );
}

#[test]
fn test_newlines_are_whitespace() {
    let tokens = tokenize("host\n127.0.0.1");
    assert_eq!(
        tokens,
        vec![
            TokenKind::Word("host".to_string()),
            TokenKind::Word("127.0.0.1".to_string()),
        ]
    );
}

#[test]
fn test_tabs_are_whitespace() {
    let tokens = tokenize("host\t127.0.0.1");
    assert_eq!(
        tokens,
        vec![
            TokenKind::Word("host".to_string()),
            TokenKind::Word("127.0.0.1".to_string()),
        ]
    );
}

#[test]
fn test_leading_trailing_whitespace() {
    let tokens = tokenize("  server  ");
    assert_eq!(tokens, vec![TokenKind::Word("server".to_string())]);
}

// ── Real config fragments ─────────────────────────────────────────────────────

#[test]
fn test_server_block_tokens() {
    let input = "server {\n    host 127.0.0.1;\n}";
    let tokens = tokenize(input);
    assert_eq!(
        tokens,
        vec![
            TokenKind::Word("server".to_string()),
            TokenKind::LBrace,
            TokenKind::Word("host".to_string()),
            TokenKind::Word("127.0.0.1".to_string()),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]
    );
}

#[test]
fn test_location_block_tokens() {
    let input = "location / {\n    root ./www;\n}";
    let tokens = tokenize(input);
    assert_eq!(
        tokens,
        vec![
            TokenKind::Word("location".to_string()),
            TokenKind::Word("/".to_string()),
            TokenKind::LBrace,
            TokenKind::Word("root".to_string()),
            TokenKind::Word("./www".to_string()),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]
    );
}

#[test]
fn test_multiple_methods_tokens() {
    let input = "methods GET POST DELETE;";
    let tokens = tokenize(input);
    assert_eq!(
        tokens,
        vec![
            TokenKind::Word("methods".to_string()),
            TokenKind::Word("GET".to_string()),
            TokenKind::Word("POST".to_string()),
            TokenKind::Word("DELETE".to_string()),
            TokenKind::Semicolon,
        ]
    );
}

#[test]
fn test_error_page_tokens() {
    let input = "error_page 404 ./error_pages/404.html;";
    let tokens = tokenize(input);
    assert_eq!(
        tokens,
        vec![
            TokenKind::Word("error_page".to_string()),
            TokenKind::Word("404".to_string()),
            TokenKind::Word("./error_pages/404.html".to_string()),
            TokenKind::Semicolon,
        ]
    );
}

#[test]
fn test_path_with_slash_is_single_token() {
    let tokens = tokenize("/images");
    assert_eq!(tokens, vec![TokenKind::Word("/images".to_string())]);
}

#[test]
fn test_relative_path_is_single_token() {
    let tokens = tokenize("./www/images");
    assert_eq!(tokens, vec![TokenKind::Word("./www/images".to_string())]);
}
