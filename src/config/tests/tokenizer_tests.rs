use crate::config::tokenizer::{ TokenKind, tokenize};

// ── Helper ────────────────────────────────────────────────────────────────────

// Strip line numbers — tests only care about token kinds, not positions
fn kinds(input: &str) -> Vec<TokenKind> {
    tokenize(input).into_iter().map(|t| t.kind).collect()
}

// ── Basic tokens ──────────────────────────────────────────────────────────────

#[test]
fn test_empty_input() {
    assert!(tokenize("").is_empty());
}

#[test]
fn test_single_word() {
    assert_eq!(kinds("server"), vec![TokenKind::Word("server".to_string())]);
}

#[test]
fn test_braces() {
    assert_eq!(kinds("{}"), vec![TokenKind::LBrace, TokenKind::RBrace]);
}

#[test]
fn test_semicolon() {
    assert_eq!(kinds(";"), vec![TokenKind::Semicolon]);
}

#[test]
fn test_word_with_semicolon() {
    assert_eq!(
        kinds("host 127.0.0.1;"),
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
    assert_eq!(
        kinds("host    127.0.0.1"),
        vec![
            TokenKind::Word("host".to_string()),
            TokenKind::Word("127.0.0.1".to_string()),
        ]
    );
}

#[test]
fn test_newlines_are_whitespace() {
    assert_eq!(
        kinds("host\n127.0.0.1"),
        vec![
            TokenKind::Word("host".to_string()),
            TokenKind::Word("127.0.0.1".to_string()),
        ]
    );
}

#[test]
fn test_tabs_are_whitespace() {
    assert_eq!(
        kinds("host\t127.0.0.1"),
        vec![
            TokenKind::Word("host".to_string()),
            TokenKind::Word("127.0.0.1".to_string()),
        ]
    );
}

#[test]
fn test_leading_trailing_whitespace() {
    assert_eq!(
        kinds("  server  "),
        vec![TokenKind::Word("server".to_string())]
    );
}

// ── Line number tracking ──────────────────────────────────────────────────────

#[test]
fn test_line_numbers_tracked() {
    let tokens = tokenize("server {\n    host 127.0.0.1;\n}");
    // "server" and "{" are on line 1
    assert_eq!(tokens[0].line, 1);
    assert_eq!(tokens[1].line, 1);
    // "host" and "127.0.0.1" and ";" are on line 2
    assert_eq!(tokens[2].line, 2);
    assert_eq!(tokens[3].line, 2);
    assert_eq!(tokens[4].line, 2);
    // "}" is on line 3
    assert_eq!(tokens[5].line, 3);
}

#[test]
fn test_line_number_starts_at_1() {
    let tokens = tokenize("server");
    assert_eq!(tokens[0].line, 1);
}

// ── Real config fragments ─────────────────────────────────────────────────────

#[test]
fn test_server_block_tokens() {
    assert_eq!(
        kinds("server {\n    host 127.0.0.1;\n}"),
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
    assert_eq!(
        kinds("location / {\n    root ./www;\n}"),
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
    assert_eq!(
        kinds("methods GET POST DELETE;"),
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
    assert_eq!(
        kinds("error_page 404 ./error_pages/404.html;"),
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
    assert_eq!(
        kinds("/images"),
        vec![TokenKind::Word("/images".to_string())]
    );
}

#[test]
fn test_relative_path_is_single_token() {
    assert_eq!(
        kinds("./www/images"),
        vec![TokenKind::Word("./www/images".to_string())]
    );
}
