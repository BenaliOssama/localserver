// src/cgi.rs

use crate::request::Request;
use crate::response::{Response, StatusCode};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

pub struct CgiRunner {
    pub interpreter: String,
    pub script_path: String,
}

impl CgiRunner {
    pub fn new(interpreter: &str, script_path: &str) -> CgiRunner {
        CgiRunner {
            interpreter: interpreter.to_string(),
            script_path: script_path.to_string(),
        }
    }

    pub fn run(&self, req: &Request) -> Response {
        // ── Build the child process ───────────────────────────────────────
        let mut child = match Command::new(&self.interpreter)
            .arg(&self.script_path)
            // ── Environment variables — the CGI contract ──────────────────
            .env("REQUEST_METHOD", method_str(&req.method))
            .env("PATH_INFO", &req.path)
            .env("QUERY_STRING", query_string(&req.path))
            .env("CONTENT_LENGTH", req.body.len().to_string())
            .env(
                "CONTENT_TYPE",
                req.headers
                    .get("content-type")
                    .map(|s| s.as_str())
                    .unwrap_or(""),
            )
            .env("SERVER_PROTOCOL", "HTTP/1.1")
            .env("GATEWAY_INTERFACE", "CGI/1.1")
            // ── Pipe stdin/stdout ─────────────────────────────────────────
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                eprintln!("Failed to spawn CGI process: {}", e);
                return Response::error(StatusCode::InternalServerError);
            }
        };

        // ── Write request body to child's stdin ───────────────────────────
        if !req.body.is_empty() {
            if let Some(stdin) = child.stdin.take() {
                let mut stdin = stdin;
                if let Err(e) = stdin.write_all(&req.body) {
                    eprintln!("Failed to write to CGI stdin: {}", e);
                }
                // stdin closes here when it drops — sends EOF to the script
            }
        }

        // ── Wait for child with timeout ───────────────────────────────────
        let output = match wait_with_timeout(child, Duration::from_secs(10)) {
            Some(output) => output,
            None => {
                eprintln!("CGI script timed out: {}", self.script_path);
                return Response::error(StatusCode::InternalServerError);
            }
        };

        if !output.status.success() {
            eprintln!(
                "CGI script exited with error: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return Response::error(StatusCode::InternalServerError);
        }

        // ── Parse CGI output ──────────────────────────────────────────────
        parse_cgi_output(&output.stdout)
    }
}

// CGI output format:
//   Header: Value\n
//   Header: Value\n
//   \n                ← blank line
//   body bytes
fn parse_cgi_output(output: &[u8]) -> Response {
    // Split on \n\n or \r\n\r\n
    let separator: &[u8] = b"\r\n\r\n";
    let alt_sep: &[u8] = b"\n\n";

    let (header_bytes, body) = if let Some(pos) = output.windows(4).position(|w| w == separator) {
        (&output[..pos], &output[pos + 4..])
    } else if let Some(pos) = output.windows(2).position(|w| w == alt_sep) {
        (&output[..pos], &output[pos + 2..])
    } else {
        // No separator found — treat entire output as body
        eprintln!("CGI output missing header/body separator");
        return Response::error(StatusCode::InternalServerError);
    };

    // Parse headers from CGI output
    let header_text = String::from_utf8_lossy(header_bytes);
    let mut content_type = "text/html".to_string();
    let mut status = StatusCode::Ok;

    for line in header_text.lines() {
        if let Some((key, value)) = line.split_once(':') {
            match key.trim().to_lowercase().as_str() {
                "content-type" => {
                    content_type = value.trim().to_string();
                }
                "status" => {
                    // CGI can set status like "Status: 404 Not Found"
                    let code = value
                        .trim()
                        .split_whitespace()
                        .next()
                        .and_then(|s| s.parse::<u16>().ok())
                        .unwrap_or(200);
                    status = match code {
                        200 => StatusCode::Ok,
                        400 => StatusCode::BadRequest,
                        403 => StatusCode::Forbidden,
                        404 => StatusCode::NotFound,
                        500 => StatusCode::InternalServerError,
                        _ => StatusCode::Ok,
                    };
                }
                _ => {} // ignore other CGI headers for now
            }
        }
    }

    Response::new(status, &content_type, body.to_vec())
}

// Wait for a child process with a timeout
// Returns None if the process timed out
fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Option<std::process::Output> {
    use std::time::Instant;

    let start = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                // Process finished — collect output
                return child.wait_with_output().ok();
            }
            Ok(None) => {
                // Still running
                if start.elapsed() > timeout {
                    // Kill it
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                eprintln!("Error waiting for CGI process: {}", e);
                return None;
            }
        }
    }
}

fn method_str(method: &crate::request::Method) -> &str {
    match method {
        crate::request::Method::Get => "GET",
        crate::request::Method::Post => "POST",
        crate::request::Method::Delete => "DELETE",
        crate::request::Method::Unknown(s) => s.as_str(),
    }
}

fn query_string(path: &str) -> &str {
    // Extract query string from path — "/cgi/hello.py?foo=bar" → "foo=bar"
    match path.split_once('?') {
        Some((_, query)) => query,
        None => "",
    }
}
