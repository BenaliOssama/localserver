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
