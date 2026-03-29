pub mod errors;
pub mod parser;
pub mod tokenizer;

#[cfg(test)]
mod tests;

use std::{collections::HashMap};

#[derive(Debug)]
pub struct Config {
    pub servers: Vec<ServerConfig>,
}

#[derive(Debug)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub server_name: Option<String>,
    pub client_max_body_size: usize,       // in bytes
    pub error_pages: HashMap<u16, String>, // e.g. 404 → "./error_pages/404.html"
    pub locations: Vec<Location>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Location {
    pub path: String,
    pub root: String,
    pub index: Option<String>,
    pub methods: Vec<Method>,
    pub autoindex: bool,
    pub redirect: Option<String>,
    pub cgi: Option<CGI>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Method {
    Get,
    Post,
    Delete,
}

#[derive(Debug)]
pub struct CGI {
    pub extension: String,
    pub interpreter: String,
}

impl Config {
    pub fn from_file(path: &str) -> Result<Config, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Cannot read config file '{}': {}", path, e))?;

        let tokens = tokenizer::tokenize(&content);
        let mut parser = parser::Parser::new(tokens);

        let config = parser.parse_config().map_err(|e| e.to_string())?;

        // Check for duplicate host:port + server_name combinations
        let mut seen = std::collections::HashSet::new();
        for server in &config.servers {
            // Two servers can share host:port only if they have different server_names
            // Two servers cannot share host:port + server_name
            let key = format!(
                "{}:{}:{}",
                server.host,
                server.port,
                server.server_name.as_deref().unwrap_or("")
            );
            if !seen.insert(key.clone()) {
                return Err(format!(
                    "Duplicate server '{}' — each host:port:server_name combination must be unique",
                    key
                ));
            }
        }
        Ok(config)
    }
}

impl ServerConfig {
    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

// Converts "10MB", "1kb", "500" etc. into bytes
pub fn parse_body_size(s: &str) -> Result<usize, String> {
    let s = s.to_lowercase();

    if let Some(n) = s.strip_suffix("mb") {
        n.trim()
            .parse::<usize>()
            .map(|n| n * 1024 * 1024)
            .map_err(|_| format!("Invalid size: {}", s))
    } else if let Some(n) = s.strip_suffix("kb") {
        n.trim()
            .parse::<usize>()
            .map(|n| n * 1024)
            .map_err(|_| format!("Invalid size: {}", s))
    } else {
        s.trim()
            .parse::<usize>()
            .map_err(|_| format!("Invalid size: {}", s))
    }
}

#[test]
fn test_duplicate_port_fails() {
    // Simulating Config::from_file behavior — we test the validation
    // by checking two servers with same host:port
    let input = r#"
        server { host 127.0.0.1; port 8080; }
        server { host 127.0.0.1; port 8080; }
    "#;

    // Parse succeeds but validation should catch duplicate
    let tokens = tokenizer::tokenize(&input); //tokenizer.toc(input);
    let mut parser = parser::Parser::new(tokens); //Parser::new(tokens);
    let config = parser.parse_config().unwrap();

    let mut seen = std::collections::HashSet::new();
    let has_duplicate = config.servers.iter().any(|s| !seen.insert(s.addr()));

    assert!(has_duplicate, "Should have detected duplicate address");
}
