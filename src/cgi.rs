// src/cgi.rs

use crate::request::Request;
use crate::response::{Response, StatusCode};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

#[allow(dead_code)]
pub struct CgiRunner {
    pub interpreter: String,
    pub script_path: String,
    pub timeout: Duration,
}

impl CgiRunner {
    pub fn new(interpreter: &str, script_path: &str) -> CgiRunner {
        CgiRunner {
            interpreter: interpreter.to_string(),
            script_path: script_path.to_string(),
            timeout: Duration::from_secs(10),
        }
    }
    #[allow(dead_code)]
    pub fn with_timeout(mut self, timeout: Duration) -> CgiRunner {
        self.timeout = timeout;
        self
    }
    pub fn run(&self, req: &Request) -> Response {
        // ── Build the child process ───────────────────────────────────────
        let mut child = match Command::new(&self.interpreter)
            .arg(&self.script_path)
            // ── Environment variables — the CGI contract ──────────────────
            .env(
                "QUERY_STRING",
                req.headers
                    .get("x-query-string")
                    .map(|s| s.as_str())
                    .unwrap_or(""),
            )
            .env("REQUEST_METHOD", method_str(&req.method))
            .env("PATH_INFO", &req.path)
            //.env("QUERY_STRING", query_string(&req.path))
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
pub(crate) fn parse_cgi_output(output: &[u8]) -> Response {
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
#[allow(dead_code)]
fn query_string(path: &str) -> &str {
    // Extract query string from path — "/cgi/hello.py?foo=bar" → "foo=bar"
    match path.split_once('?') {
        Some((_, query)) => query,
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response::StatusCode;

    // ── parse_cgi_output ──────────────────────────────────────────────────

    #[test]
    fn test_basic_html_response() {
        let output = b"Content-Type: text/html\r\n\r\n<h1>Hello</h1>";
        let res = parse_cgi_output(output);
        assert_eq!(res.content_type, "text/html");
        assert_eq!(res.body, b"<h1>Hello</h1>");
    }

    #[test]
    fn test_unix_line_endings() {
        // Python's print() uses \n not \r\n
        let output = b"Content-Type: text/html\n\n<h1>Hello</h1>";
        let res = parse_cgi_output(output);
        assert_eq!(res.content_type, "text/html");
        assert_eq!(res.body, b"<h1>Hello</h1>");
    }

    #[test]
    fn test_default_content_type_is_html() {
        // No Content-Type header — should default to text/html
        let output = b"Content-Type: text/html\r\n\r\nbody";
        let res = parse_cgi_output(output);
        assert_eq!(res.content_type, "text/html");
    }

    #[test]
    fn test_custom_content_type() {
        let output = b"Content-Type: application/json\r\n\r\n{\"ok\":true}";
        let res = parse_cgi_output(output);
        assert_eq!(res.content_type, "application/json");
        assert_eq!(res.body, b"{\"ok\":true}");
    }

    #[test]
    fn test_status_200_default() {
        let output = b"Content-Type: text/html\r\n\r\nok";
        let res = parse_cgi_output(output);
        assert!(matches!(res.status, StatusCode::Ok));
    }

    #[test]
    fn test_status_header_404() {
        let output = b"Content-Type: text/html\r\nStatus: 404 Not Found\r\n\r\nnot found";
        let res = parse_cgi_output(output);
        assert!(matches!(res.status, StatusCode::NotFound));
    }

    #[test]
    fn test_status_header_403() {
        let output = b"Content-Type: text/html\r\nStatus: 403 Forbidden\r\n\r\nforbidden";
        let res = parse_cgi_output(output);
        assert!(matches!(res.status, StatusCode::Forbidden));
    }

    #[test]
    fn test_status_header_500() {
        let output = b"Content-Type: text/html\r\nStatus: 500 Internal Server Error\r\n\r\nerror";
        let res = parse_cgi_output(output);
        assert!(matches!(res.status, StatusCode::InternalServerError));
    }

    #[test]
    fn test_empty_body() {
        let output = b"Content-Type: text/html\r\n\r\n";
        let res = parse_cgi_output(output);
        assert!(res.body.is_empty());
    }

    #[test]
    fn test_binary_body_survives() {
        let mut output = b"Content-Type: application/octet-stream\r\n\r\n".to_vec();
        output.extend_from_slice(&[0xFF, 0xFE, 0x00, 0x01]);
        let res = parse_cgi_output(&output);
        assert_eq!(res.body, &[0xFF, 0xFE, 0x00, 0x01]);
    }

    #[test]
    fn test_missing_separator_returns_500() {
        // No \r\n\r\n or \n\n — malformed CGI output
        let output = b"Content-Type: text/html body with no separator";
        let res = parse_cgi_output(output);
        assert!(matches!(res.status, StatusCode::InternalServerError));
    }

    #[test]
    fn test_body_with_blank_lines() {
        // Body itself contains blank lines — must not confuse the parser
        let output = b"Content-Type: text/html\r\n\r\nline1\r\n\r\nline2";
        let res = parse_cgi_output(output);
        assert_eq!(res.body, b"line1\r\n\r\nline2");
    }

    #[test]
    fn test_multiple_headers() {
        let output = b"Content-Type: text/plain\r\nStatus: 200 OK\r\nX-Custom: value\r\n\r\nhello";
        let res = parse_cgi_output(output);
        assert_eq!(res.content_type, "text/plain");
        assert_eq!(res.body, b"hello");
    }

    // ── CgiRunner integration ─────────────────────────────────────────────
    // These tests actually run a real Python script

    #[test]
    fn test_real_cgi_script_get() {
        // Skip if python3 not available
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }

        // Write a minimal test script
        let script = "/tmp/test_cgi.py";
        std::fs::write(script, b"#!/usr/bin/env python3\nprint('Content-Type: text/plain')\nprint()\nprint('hello from cgi')\n").unwrap();

        use crate::request::{Method, Request};
        let req = Request {
            method: Method::Get,
            path: "/test.py".to_string(),
            version: "HTTP/1.1".to_string(),
            headers: std::collections::HashMap::new(),
            body: Vec::new(),
        };

        let runner = CgiRunner::new("python3", script);
        let res = runner.run(&req);

        assert!(matches!(res.status, StatusCode::Ok));
        assert_eq!(res.content_type, "text/plain");
        assert_eq!(res.body, b"hello from cgi\n");
    }

    #[test]
    fn test_real_cgi_script_reads_env() {
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }

        let script = "/tmp/test_cgi_env.py";
        std::fs::write(script,
            b"#!/usr/bin/env python3\nimport os\nprint('Content-Type: text/plain')\nprint()\nprint(os.environ.get('REQUEST_METHOD', 'MISSING'))\n"
        ).unwrap();

        use crate::request::{Method, Request};
        let req = Request {
            method: Method::Post,
            path: "/test.py".to_string(),
            version: "HTTP/1.1".to_string(),
            headers: std::collections::HashMap::new(),
            body: Vec::new(),
        };

        let runner = CgiRunner::new("python3", script);
        let res = runner.run(&req);
        assert_eq!(res.body, b"POST\n");
    }

    #[test]
    fn test_cgi_timeout() {
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }

        // Script that sleeps forever
        let script = "/tmp/test_cgi_timeout.py";
        std::fs::write(
            script,
            b"#!/usr/bin/env python3\nimport time\ntime.sleep(999)\n",
        )
        .unwrap();

        use crate::request::{Method, Request};
        let req = Request {
            method: Method::Get,
            path: "/test.py".to_string(),
            version: "HTTP/1.1".to_string(),
            headers: std::collections::HashMap::new(),
            body: Vec::new(),
        };

        // Use a very short timeout for testing
        let runner = CgiRunner::new("python3", script).with_timeout(Duration::from_millis(200));

        let res = runner.run(&req);

        // Timed out script must return 500 not hang
        assert!(matches!(res.status, StatusCode::InternalServerError));
    }

    #[test]
    fn test_nonexistent_script_returns_500() {
        use crate::request::{Method, Request};
        let req = Request {
            method: Method::Get,
            path: "/ghost.py".to_string(),
            version: "HTTP/1.1".to_string(),
            headers: std::collections::HashMap::new(),
            body: Vec::new(),
        };

        let runner = CgiRunner::new("python3", "/tmp/does_not_exist.py");
        let res = runner.run(&req);
        assert!(matches!(res.status, StatusCode::InternalServerError));
    }
}
