// tests/common/mod.rs

use localserver::config::{CGI, Location, Method, ServerConfig};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

// ── Network helpers ───────────────────────────────────────────────────────────

pub fn send_request(port: u16, request: &str) -> Vec<u8> {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();

    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    buf
}

pub fn status_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .next()
        .unwrap_or("")
        .to_string()
}

pub fn body(bytes: &[u8]) -> Vec<u8> {
    let sep = b"\r\n\r\n";
    match bytes.windows(4).position(|w| w == sep) {
        Some(pos) => bytes[pos + 4..].to_vec(),
        None => Vec::new(),
    }
}

pub fn header(bytes: &[u8], name: &str) -> Option<String> {
    let raw = String::from_utf8_lossy(bytes);
    for line in raw.lines() {
        if line.to_lowercase().starts_with(&name.to_lowercase()) {
            return Some(line.splitn(2, ':').nth(1)?.trim().to_string());
        }
    }
    None
}

// ── Server factories ──────────────────────────────────────────────────────────

// Grab a free port from the OS
pub fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

// Start a basic static file server
pub fn start_server() -> u16 {
    let port = free_port();

    let config = ServerConfig {
        host: "127.0.0.1".to_string(),
        port,
        server_name: None,
        client_max_body_size: 1024 * 1024,
        error_pages: std::collections::HashMap::new(),
        locations: vec![
            Location {
                path: "/".to_string(),
                root: "./www".to_string(),
                index: Some("index.html".to_string()),
                methods: vec![],
                autoindex: false,
                redirect: None,
                cgi: None,
            },
            Location {
                path: "/uploads".to_string(),
                root: "./www".to_string(),
                index: None,
                methods: vec![Method::Get, Method::Post, Method::Delete],
                autoindex: false,
                redirect: None,
                cgi: None,
            },
        ],
    };

    spawn_server(config);
    port
}

// Start a CGI-enabled server
pub fn start_cgi_server() -> u16 {
    let port = free_port();

    std::fs::create_dir_all("/tmp/e2e_cgi_bin").unwrap();

    std::fs::write(
        "/tmp/e2e_cgi_bin/hello.py",
        b"#!/usr/bin/env python3
import os, sys
method  = os.environ.get('REQUEST_METHOD', '')
query   = os.environ.get('QUERY_STRING', '')
length  = int(os.environ.get('CONTENT_LENGTH', '0'))
body    = sys.stdin.read(length) if length > 0 else ''
print('Content-Type: text/plain')
print()
print(f'method={method}')
print(f'query={query}')
print(f'body={body}')
",
    )
    .unwrap();

    std::fs::write(
        "/tmp/e2e_cgi_bin/status.py",
        b"#!/usr/bin/env python3
print('Content-Type: text/html')
print('Status: 404 Not Found')
print()
print('<h1>Custom 404</h1>')
",
    )
    .unwrap();

    std::fs::write(
        "/tmp/e2e_cgi_bin/hang.py",
        b"#!/usr/bin/env python3
import time
time.sleep(999)
",
    )
    .unwrap();

    std::fs::write(
        "/tmp/e2e_cgi_bin/crash.py",
        b"#!/usr/bin/env python3
raise Exception('intentional crash')
",
    )
    .unwrap();

    let config = ServerConfig {
        host: "127.0.0.1".to_string(),
        port,
        server_name: None,
        client_max_body_size: 1024 * 1024,
        error_pages: std::collections::HashMap::new(),
        locations: vec![Location {
            path: "/cgi".to_string(),
            root: "/tmp/e2e_cgi_bin".to_string(),
            index: None,
            methods: vec![Method::Get, Method::Post],
            autoindex: false,
            redirect: None,
            cgi: Some(CGI {
                extension: ".py".to_string(),
                interpreter: "python3".to_string(),
            }),
        }],
    };

    spawn_server(config);
    port
}

// Internal helper — spawns a server thread and waits for it to start
fn spawn_server(config: ServerConfig) {
    thread::spawn(move || {
        localserver::server::Server::new(config).run().unwrap();
    });
    thread::sleep(Duration::from_millis(100));
}
