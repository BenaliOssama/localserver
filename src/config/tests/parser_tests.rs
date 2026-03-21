use crate::config::parser::Parser;
use crate::config::tokenizer::tokenize;
use crate::config::{Config, Method};

// ── Helper ────────────────────────────────────────────────────────────────────

fn parse(input: &str) -> Result<Config, crate::errors::ParseError> {
    let tokens = tokenize(input);
    Parser::new(tokens).parse_config()
}

// ── Valid configs ─────────────────────────────────────────────────────────────

#[test]
fn test_minimal_server() {
    let input = r#"
        server {
            host 127.0.0.1;
            port 8080;
        }
    "#;
    let config = parse(input).expect("should parse");
    assert_eq!(config.servers.len(), 1);
    assert_eq!(config.servers[0].host, "127.0.0.1");
    assert_eq!(config.servers[0].port, 8080);
}
