// src/handler.rs

use crate::config::{Location, Method as ConfigMethod, ServerConfig};
use crate::request::{Method, Request};
use crate::response::{Response, StatusCode};
use std::fs;
use std::net::TcpStream;

fn get_content_type(path: &str) -> &str {
    if path.ends_with(".html") {
        "text/html"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") {
        "image/jpeg"
    } else if path.ends_with(".json") {
        "application/json"
    } else if path.ends_with(".txt") {
        "text/plain"
    } else {
        "application/octet-stream"
    }
}

// Find the most specific matching location for a given path
fn match_location<'a>(path: &str, config: &'a ServerConfig) -> Option<&'a Location> {
    config
        .locations
        .iter()
        .filter(|loc| path.starts_with(&loc.path))
        .max_by_key(|loc| loc.path.len()) // longest match wins
}

fn is_method_allowed(req_method: &Method, loc: &Location) -> bool {
    // If no methods specified in config, allow all
    if loc.methods.is_empty() {
        return true;
    }
    loc.methods.iter().any(|m| {
        matches!(
            (m, req_method),
            (ConfigMethod::Get, Method::Get)
                | (ConfigMethod::Post, Method::Post)
                | (ConfigMethod::Delete, Method::Delete)
        )
    })
}

fn error_response(status: StatusCode, config: &ServerConfig) -> Response {
    // Check if config defines a custom error page for this status
    let code = match status {
        StatusCode::NotFound => 404u16,
        StatusCode::Forbidden => 403,
        StatusCode::InternalServerError => 500,
        StatusCode::BadRequest => 400,
        StatusCode::MethodNotAllowed => 405,
        StatusCode::ContentTooLarge => 413,
        StatusCode::Ok => return Response::error(status),
    };

    if let Some(page_path) = config.error_pages.get(&code) {
        if let Ok(contents) = fs::read(page_path) {
            return Response::new(status, "text/html", contents);
        }
    }

    // Fall back to default error page
    Response::error(status)
}

fn serve_file(path: &str, root: &str, config: &ServerConfig) -> Response {
    let normalized = if path.ends_with('/') {
        format!("{}index.html", path)
    } else {
        path.to_string()
    };

    let file_path = format!("{}{}", root, normalized);

    match fs::read(&file_path) {
        Ok(contents) => {
            let content_type = get_content_type(&normalized);
            Response::new(StatusCode::Ok, content_type, contents)
        }
        Err(_) => error_response(StatusCode::NotFound, config),
    }
}

fn handle_post(req: &Request, root: &str, config: &ServerConfig) -> Response {
    if req.body.is_empty() {
        return error_response(StatusCode::BadRequest, config);
    }

    let file_path = format!("{}{}", root, req.path);

    if let Some(parent) = std::path::Path::new(&file_path).parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("Failed to create directory: {}", e);
            return error_response(StatusCode::InternalServerError, config);
        }
    }

    match fs::write(&file_path, &req.body) {
        Ok(_) => {
            let body = format!(
                "<html><body><h1>Uploaded to {}</h1></body></html>",
                req.path
            );
            Response::new(StatusCode::Ok, "text/html", body.into_bytes())
        }
        Err(e) => {
            eprintln!("Failed to write file: {}", e);
            error_response(StatusCode::InternalServerError, config)
        }
    }
}

fn handle_delete(req: &Request, root: &str, config: &ServerConfig) -> Response {
    let file_path = format!("{}{}", root, req.path);

    match fs::remove_file(&file_path) {
        Ok(_) => {
            let body = format!("<html><body><h1>Deleted {}</h1></body></html>", req.path);
            Response::new(StatusCode::Ok, "text/html", body.into_bytes())
        }
        Err(_) => error_response(StatusCode::NotFound, config),
    }
}

pub fn handle(req: Request, stream: &mut TcpStream, config: &ServerConfig) {
    use std::io::Write;
    // ── Check for redirect first ──────────────────────────────────────────
    if let Some(loc) = match_location(&req.path, config) {
        if let Some(redirect_to) = &loc.redirect {
            let body = format!(
                "<html><body>Redirecting to <a href='{}'>{}</a></body></html>",
                redirect_to, redirect_to
            );
            let response = format!(
                "HTTP/1.1 301 Moved Permanently\r\nLocation: {}\r\nContent-Length: {}\r\nContent-Type: text/html\r\n\r\n{}",
                redirect_to,
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            return;
        }
    }

    // ── Match location and check method ───────────────────────────────────
    let response = match match_location(&req.path, config) {
        None => error_response(StatusCode::NotFound, config),

        Some(loc) => {
            if !is_method_allowed(&req.method, loc) {
                error_response(StatusCode::MethodNotAllowed, config)
            } else {
                let root = loc.root.clone();
                match req.method {
                    Method::Get => serve_file(&req.path, &root, config),
                    Method::Post => handle_post(&req, &root, config),
                    Method::Delete => handle_delete(&req, &root, config),
                    Method::Unknown(_) => error_response(StatusCode::MethodNotAllowed, config),
                }
            }
        }
    };

    response.send(stream);
}

// Keep the test helper working
pub fn handle_with_root(req: Request, stream: &mut TcpStream, root: &str) {
    let mut config = ServerConfig {
        host: "127.0.0.1".to_string(),
        port: 8080,
        client_max_body_size: 1024 * 1024,
        error_pages: std::collections::HashMap::new(),
        locations: vec![crate::config::Location {
            path: "/".to_string(),
            root: root.to_string(),
            index: Some("index.html".to_string()),
            methods: vec![],
            autoindex: false,
            redirect: None,
            cgi: None,
        }],
    };
    handle(req, stream, &config);
}
