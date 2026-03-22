use crate::config::errors::ParseError;
use crate::config::parser::Parser;
use crate::config::tokenizer::tokenize;
use crate::config::{Config, Method};

// ── Helper ────────────────────────────────────────────────────────────────────

fn parse(input: &str) -> Result<Config, ParseError> {
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

#[test]
fn test_multiple_servers() {
    let input = r#"
        server { host 127.0.0.1; port 8080; }
        server { host 127.0.0.2; port 8081; }
        server { host 127.0.0.3; port 8082; }
    "#;
    let config = parse(input).expect("should parse");
    assert_eq!(config.servers.len(), 3);
    assert_eq!(config.servers[1].host, "127.0.0.2");
    assert_eq!(config.servers[2].port, 8082);
}

#[test]
fn test_client_max_body_size_mb() {
    let input = r#"
        server {
            host 127.0.0.1;
            port 8080;
            client_max_body_size 20MB;
        }
    "#;
    let config = parse(input).expect("should parse");
    assert_eq!(config.servers[0].client_max_body_size, 20 * 1024 * 1024);
}

#[test]
fn test_client_max_body_size_kb() {
    let input = r#"
        server {
            host 127.0.0.1;
            port 8080;
            client_max_body_size 512KB;
        }
    "#;
    let config = parse(input).expect("should parse");
    assert_eq!(config.servers[0].client_max_body_size, 512 * 1024);
}

#[test]
fn test_client_max_body_size_defaults_to_1mb() {
    let input = r#"
        server { host 127.0.0.1; port 8080; }
    "#;
    let config = parse(input).expect("should parse");
    assert_eq!(config.servers[0].client_max_body_size, 1024 * 1024);
}

#[test]
fn test_error_pages_parsed() {
    let input = r#"
        server {
            host 127.0.0.1;
            port 8080;
            error_page 404 ./errors/404.html;
            error_page 500 ./errors/500.html;
            error_page 403 ./errors/403.html;
        }
    "#;
    let config = parse(input).expect("should parse");
    let pages = &config.servers[0].error_pages;

    assert_eq!(
        pages.get(&404).map(|s| s.as_str()),
        Some("./errors/404.html")
    );
    assert_eq!(
        pages.get(&500).map(|s| s.as_str()),
        Some("./errors/500.html")
    );
    assert_eq!(
        pages.get(&403).map(|s| s.as_str()),
        Some("./errors/403.html")
    );
}

#[test]
fn test_server_addr() {
    let input = r#"
        server { host 127.0.0.1; port 8080; }
    "#;
    let config = parse(input).expect("should parse");
    assert_eq!(config.servers[0].addr(), "127.0.0.1:8080");
}

#[test]
fn test_location_root() {
    let input = r#"
        server {
            host 127.0.0.1;
            port 8080;
            location / {
                root ./www;
                methods GET;
            }
        }
    "#;
    let config = parse(input).expect("should parse");
    let loc = &config.servers[0].locations[0];
    assert_eq!(loc.path, "/");
    assert_eq!(loc.root, "./www");
}

#[test]
fn test_location_index() {
    let input = r#"
        server {
            host 127.0.0.1;
            port 8080;
            location / {
                root ./www;
                index index.html;
                methods GET;
            }
        }
    "#;
    let config = parse(input).expect("should parse");
    let loc = &config.servers[0].locations[0];
    assert_eq!(loc.index.as_deref(), Some("index.html"));
}

#[test]
fn test_location_methods_get() {
    let input = r#"
        server {
            host 127.0.0.1; port 8080;
            location / { root ./www; methods GET; }
        }
    "#;
    let config = parse(input).expect("should parse");
    let methods = &config.servers[0].locations[0].methods;
    assert_eq!(methods.len(), 1);
    assert!(matches!(methods[0], Method::Get));
}

#[test]
fn test_location_multiple_methods() {
    let input = r#"
        server {
            host 127.0.0.1; port 8080;
            location /upload { root ./uploads; methods GET POST DELETE; }
        }
    "#;
    let config = parse(input).expect("should parse");
    let methods = &config.servers[0].locations[0].methods;
    assert_eq!(methods.len(), 3);
    assert!(matches!(methods[0], Method::Get));
    assert!(matches!(methods[1], Method::Post));
    assert!(matches!(methods[2], Method::Delete));
}

#[test]
fn test_location_autoindex_on() {
    let input = r#"
        server {
            host 127.0.0.1; port 8080;
            location /files { root ./files; autoindex on; methods GET; }
        }
    "#;
    let config = parse(input).expect("should parse");
    assert!(config.servers[0].locations[0].autoindex);
}

#[test]
fn test_location_autoindex_off() {
    let input = r#"
        server {
            host 127.0.0.1; port 8080;
            location / { root ./www; autoindex off; methods GET; }
        }
    "#;
    let config = parse(input).expect("should parse");
    assert!(!config.servers[0].locations[0].autoindex);
}

#[test]
fn test_location_redirect() {
    let input = r#"
        server {
            host 127.0.0.1; port 8080;
            location /old { redirect /new; }
        }
    "#;
    let config = parse(input).expect("should parse");
    let loc = &config.servers[0].locations[0];
    assert_eq!(loc.redirect.as_deref(), Some("/new"));
}

#[test]
fn test_location_cgi() {
    let input = r#"
        server {
            host 127.0.0.1; port 8080;
            location /cgi { root ./cgi-bin; methods GET POST; cgi .py python3; }
        }
    "#;
    let config = parse(input).expect("should parse");
    let cgi = config.servers[0].locations[0].cgi.as_ref().unwrap();
    assert_eq!(cgi.extension, ".py");
    assert_eq!(cgi.interpreter, "python3");
}

#[test]
fn test_full_config_file() {
    let input = std::fs::read_to_string("config.conf").expect("config.conf must exist");
    let result = parse(&input);
    assert!(
        result.is_ok(),
        "config.conf should parse cleanly: {:?}",
        result
    );
}

// ── Error cases ───────────────────────────────────────────────────────────────

#[test]
fn test_empty_config_fails() {
    let result = parse("");
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("at least one server"));
}

#[test]
fn test_missing_host_fails() {
    let input = r#"server { port 8080; }"#;
    let result = parse(input);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("host"));
}

#[test]
fn test_missing_port_fails() {
    let input = r#"server { host 127.0.0.1; }"#;
    let result = parse(input);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("port"));
}

#[test]
fn test_invalid_port_fails() {
    let input = r#"server { host 127.0.0.1; port banana; }"#;
    let result = parse(input);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("port"));
}

#[test]
fn test_unknown_server_directive_fails() {
    let input = r#"server { host 127.0.0.1; port 8080; unknown_thing on; }"#;
    let result = parse(input);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("unknown_thing"));
}

#[test]
fn test_unknown_location_directive_fails() {
    let input = r#"
        server {
            host 127.0.0.1; port 8080;
            location / { root ./www; banana on; }
        }
    "#;
    let result = parse(input);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("banana"));
}

#[test]
fn test_invalid_method_fails() {
    let input = r#"
        server {
            host 127.0.0.1; port 8080;
            location / { root ./www; methods GET PATCH; }
        }
    "#;
    let result = parse(input);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("PATCH"));
}

#[test]
fn test_invalid_autoindex_value_fails() {
    let input = r#"
        server {
            host 127.0.0.1; port 8080;
            location / { root ./www; autoindex yes; }
        }
    "#;
    let result = parse(input);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("autoindex"));
}

#[test]
fn test_unknown_top_level_directive_fails() {
    let input = r#"banana { }"#;
    let result = parse(input);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("banana"));
}

#[test]
fn test_unclosed_server_block_fails() {
    let input = r#"server { host 127.0.0.1; port 8080;"#;
    let result = parse(input);
    assert!(result.is_err());
}
