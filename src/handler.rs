// src/handler.rs

use crate::cgi::CgiRunner;
use crate::config::{Location, Method as ConfigMethod, ServerConfig};
use crate::request::{Method, Request};
use crate::response::{Response, StatusCode};
use crate::utils::session;
use std::collections::HashMap;
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

fn serve_directory(url_path: &str, dir_path: &str, config: &ServerConfig) -> Response {
    // Check if autoindex is enabled for this location
    let autoindex = config
        .locations
        .iter()
        .filter(|loc| url_path.starts_with(&loc.path))
        .max_by_key(|loc| loc.path.len())
        .map(|loc| loc.autoindex)
        .unwrap_or(false);

    if !autoindex {
        return error_response(StatusCode::Forbidden, config);
    }

    // Read directory entries
    let entries = match fs::read_dir(dir_path) {
        Ok(e) => e,
        Err(_) => return error_response(StatusCode::InternalServerError, config),
    };

    // Build the HTML listing
    let mut rows = String::new();

    // Add parent directory link unless we're at root
    if url_path != "/" {
        let parent = std::path::Path::new(url_path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("/");
        rows.push_str(&format!(
            "<tr><td><a href='{}/'>..</a></td><td>-</td><td>-</td></tr>\n",
            parent
        ));
    }

    // Collect and sort entries alphabetically
    let mut entry_list: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    entry_list.sort_by_key(|e| e.file_name());

    for entry in entry_list {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let (display_name, size) = if meta.is_dir() {
            (format!("{}/", name_str), "-".to_string())
        } else {
            (name_str.to_string(), format!("{} bytes", meta.len()))
        };

        let href = format!("{}/{}", url_path.trim_end_matches('/'), display_name);

        rows.push_str(&format!(
            "<tr><td><a href='{}'>{}</a></td><td>{}</td></tr>\n",
            href, display_name, size
        ));
    }

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Index of {path}</title>
    <style>
        body  {{ font-family: monospace; padding: 2rem; }}
        h1    {{ border-bottom: 1px solid #ccc; padding-bottom: 0.5rem; }}
        table {{ border-collapse: collapse; width: 100%; }}
        td    {{ padding: 0.3rem 1rem; }}
        tr:hover {{ background: #f5f5f5; }}
        a     {{ text-decoration: none; color: #0066cc; }}
        a:hover {{ text-decoration: underline; }}
    </style>
</head>
<body>
    <h1>Index of {path}</h1>
    <table>
        <tr><th align='left'>Name</th><th align='left'>Size</th></tr>
        {rows}
    </table>
</body>
</html>"#,
        path = url_path,
        rows = rows
    );

    Response::new(StatusCode::Ok, "text/html", html.into_bytes())
}

fn serve_file(path: &str, root: &str, config: &ServerConfig) -> Response {
    let normalized = if path.ends_with('/') {
        format!("{}index.html", path)
    } else {
        path.to_string()
    };

    let file_path = format!("{}{}", root, normalized);
    let meta = match fs::metadata(&file_path) {
        Ok(m) => m,
        Err(_) => return error_response(StatusCode::NotFound, config),
    };

    // If it's a directory, hand off to directory listing
    if meta.is_dir() {
        return serve_directory(path, &file_path, config);
    }

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

pub fn build_response(req: Request, config: &ServerConfig) -> Response {
    // ── Check for redirect ────────────────────────────────────────────
    if let Some(loc) = match_location(&req.path, config) {
        if let Some(redirect_to) = &loc.redirect {
            let body = format!(
                "<html><body>Redirecting to <a href='{}'>{}</a></body></html>",
                redirect_to, redirect_to
            );
            let header = format!(
                "HTTP/1.1 301 Moved Permanently\r\nLocation: {}\r\nContent-Length: {}\r\nContent-Type: text/html\r\n\r\n{}",
                redirect_to,
                body.len(),
                body
            );
            // Return as raw response — redirect needs Location header
            return Response::redirect(redirect_to, body.into_bytes());
        }
    }

    // ── Session routes ────────────────────────────────────────────────
    match req.path.as_str() {
        "/login" if matches!(req.method, Method::Post) => handle_login(&req),
        "/logout" if matches!(req.method, Method::Post) => handle_logout(&req),
        "/whoami" => handle_whoami(&req, config),
        _ => handle_with_route(req, config),
    }
}

fn handle_with_route(req: Request, config: &ServerConfig) -> Response {
    match match_location(&req.path, config) {
        None => error_response(StatusCode::NotFound, config),
        Some(loc) => {
            if !is_method_allowed(&req.method, loc) {
                error_response(StatusCode::MethodNotAllowed, config)
            } else {
                let root = loc.root.clone();
                match req.method {
                    Method::Get | Method::Post => {
                        if let Some(cgi) = find_cgi(&req.path, loc) {
                            let relative = strip_location_prefix(&req.path, &loc.path);
                            let script = format!("{}/{}", root, relative);
                            CgiRunner::new(&cgi.interpreter, &script).run(&req)
                        } else {
                            match req.method {
                                Method::Get => {
                                    let relative = strip_location_prefix(&req.path, &loc.path);
                                    serve_file(&format!("/{}", relative), &root, config)
                                }
                                Method::Post => {
                                    let relative = strip_location_prefix(&req.path, &loc.path);
                                    let mut adjusted = req.clone();
                                    adjusted.path = format!("/{}", relative);
                                    handle_post(&adjusted, &root, config)
                                }
                                _ => unreachable!(),
                            }
                        }
                    }
                    Method::Delete => {
                        let relative = strip_location_prefix(&req.path, &loc.path);
                        let mut adjusted = req.clone();
                        adjusted.path = format!("/{}", relative);
                        handle_delete(&adjusted, &root, config)
                    }
                    Method::Unknown(_) => error_response(StatusCode::MethodNotAllowed, config),
                }
            }
        }
    }
}
fn handle_login(req: &Request) -> Response {
    println!("-----------> in the login handler");
    // Parse body as "username=sam&password=secret"
    let body = String::from_utf8_lossy(&req.body);
    let mut params: HashMap<&str, &str> = HashMap::new();
    for pair in body.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            params.insert(k, v);
        }
    }

    let username = params.get("username").copied().unwrap_or("");
    let password = params.get("password").copied().unwrap_or("");

    // Hardcoded for now — replace with real auth later
    if username == "admin" && password == "secret" {
        let mut store = session::store().lock().unwrap();
        let session_id = store.create();
        store.set(&session_id, "username", username);

        let body = b"<html><body><h1>Logged in!</h1></body></html>".to_vec();
        Response::new(StatusCode::Ok, "text/html", body).set_cookie("session_id", &session_id)
    } else {
        let body = b"<html><body><h1>Invalid credentials</h1></body></html>".to_vec();
        Response::new(StatusCode::Ok, "text/html", body)
    }
}

fn handle_logout(req: &Request) -> Response {
    if let Some(id) = req.session_id() {
        session::store().lock().unwrap().destroy(&id);
    }
    let body = b"<html><body><h1>Logged out</h1></body></html>".to_vec();
    // Expire the cookie by setting max-age=0
    Response::new(StatusCode::Ok, "text/html", body).set_cookie("session_id", "deleted; Max-Age=0")
}

fn handle_whoami(req: &Request, config: &ServerConfig) -> Response {
    let username = req
        .session_id()
        .and_then(|id| session::store().lock().unwrap().get_value(&id, "username"));

    match username {
        Some(name) => {
            let body = format!("<html><body><h1>You are: {}</h1></body></html>", name);
            Response::new(StatusCode::Ok, "text/html", body.into_bytes())
        }
        None => error_response(StatusCode::Forbidden, config),
    }
}
// Keep the test helper working
pub fn handle_with_root(req: &mut Request, stream: &mut TcpStream, root: &str) {
    let mut config = ServerConfig {
        host: "127.0.0.1".to_string(),
        port: 8080,
        server_name: None,
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
    let response = build_response(req.clone(), &config);
    response.send(stream);
}

fn find_cgi<'a>(path: &str, loc: &'a crate::config::Location) -> Option<&'a crate::config::CGI> {
    loc.cgi
        .as_ref()
        .filter(|cgi| path.ends_with(&cgi.extension))
}

fn strip_location_prefix<'a>(url_path: &'a str, location_path: &str) -> &'a str {
    url_path
        .strip_prefix(location_path)
        .unwrap_or(url_path)
        .trim_start_matches('/')
}
