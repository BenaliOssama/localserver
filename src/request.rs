// src/request.rs

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Method {
    Get,
    Post,
    Delete,
    Unknown(String),
}

#[derive(Debug, Clone)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub version: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn parse(buffer: &[u8]) -> Option<Request> {
        // Split headers and body on the blank line
        let separator = b"\r\n\r\n";
        let header_end = buffer.windows(4).position(|w| w == separator)?;

        let header_section = &buffer[..header_end];
        let body = buffer[header_end + 4..].to_vec();

        // Parse the header section as text
        let header_text = String::from_utf8_lossy(header_section);
        let mut lines = header_text.lines();

        // First line is the request line
        let first_line = lines.next()?;
        let mut parts = first_line.split_whitespace();

        let method = match parts.next()? {
            "GET" => Method::Get,
            "POST" => Method::Post,
            "DELETE" => Method::Delete,
            other => Method::Unknown(other.to_string()),
        };

        let path = parts.next()?.to_string();
        let version = parts.next()?.to_string();

        // Remaining lines are headers — "Key: Value"
        let mut headers = HashMap::new();
        for line in lines {
            if let Some((key, value)) = line.split_once(':') {
                headers.insert(key.trim().to_lowercase(), value.trim().to_string());
            }
        }

        // Check for chunked transfer encoding
        let body = if headers
            .get("transfer-encoding")
            .map(|v| v.to_lowercase().contains("chunked"))
            .unwrap_or(false)
        {
            // Decode chunked body — return None if malformed
            decode_chunked(&body)?
        } else {
            body
        };
        Some(Request {
            method,
            path,
            version,
            headers,
            body,
        })
    }
    pub fn cookies(&self) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();

        let cookie_header = match self.headers.get("cookie") {
            Some(v) => v,
            None => return map,
        };

        // "session_id=abc123; user=sam" → [("session_id", "abc123"), ("user", "sam")]
        for pair in cookie_header.split(';') {
            if let Some((key, value)) = pair.trim().split_once('=') {
                map.insert(key.trim().to_string(), value.trim().to_string());
            }
        }

        map
    }

    pub fn session_id(&self) -> Option<String> {
        self.cookies().get("session_id").cloned()
    }

    pub fn content_length(&self) -> usize {
        self.headers
            .get("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }
}

fn decode_chunked(data: &[u8]) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    let mut pos = 0;

    loop {
        // Find the end of the chunk size line (\r\n)
        let line_end = data[pos..].windows(2).position(|w| w == b"\r\n")?;

        // Parse chunk size — it's hex encoded
        let size_str = std::str::from_utf8(&data[pos..pos + line_end]).ok()?;
        let chunk_size = usize::from_str_radix(size_str.trim(), 16).ok()?;

        pos += line_end + 2; // skip past \r\n

        // Size 0 means end of body
        if chunk_size == 0 {
            break;
        }

        // Make sure we have enough data
        if pos + chunk_size > data.len() {
            return None;
        }

        // Append chunk data
        body.extend_from_slice(&data[pos..pos + chunk_size]);
        pos += chunk_size + 2; // skip past chunk data and its trailing \r\n
    }

    Some(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Valid requests ────────────────────────────────────────────────────

    #[test]
    fn test_parse_get_root() {
        let raw = b"GET / HTTP/1.1\r\nHost: localhost:8080\r\n\r\n";
        let req = Request::parse(raw).expect("should parse");

        assert!(matches!(req.method, Method::Get));
        assert_eq!(req.path, "/");
        assert_eq!(req.version, "HTTP/1.1");
    }

    #[test]
    fn test_parse_get_with_path() {
        let raw = b"GET /about.html HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let req = Request::parse(raw).expect("should parse");

        assert!(matches!(req.method, Method::Get));
        assert_eq!(req.path, "/about.html");
    }

    #[test]
    fn test_parse_post() {
        let body = "hello world";
        let raw = format!(
            "POST /upload HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let req = Request::parse(raw.as_bytes()).expect("should parse");

        assert!(matches!(req.method, Method::Post));
        assert_eq!(req.path, "/upload");
        assert_eq!(req.body, body.as_bytes());
    }

    #[test]
    fn test_parse_delete() {
        let raw = b"DELETE /file.txt HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let req = Request::parse(raw).expect("should parse");

        assert!(matches!(req.method, Method::Delete));
        assert_eq!(req.path, "/file.txt");
    }

    #[test]
    fn test_parse_unknown_method() {
        let raw = b"PATCH /file.txt HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let req = Request::parse(raw).expect("should parse");

        assert!(matches!(req.method, Method::Unknown(_)));
        if let Method::Unknown(m) = req.method {
            assert_eq!(m, "PATCH");
        }
    }
    // ── Headers ───────────────────────────────────────────────────────────

    #[test]
    fn test_headers_are_parsed() {
        let raw = b"GET / HTTP/1.1\r\nHost: localhost:8080\r\nConnection: keep-alive\r\n\r\n";
        let req = Request::parse(raw).expect("should parse");

        assert_eq!(
            req.headers.get("host").map(|s| s.as_str()),
            Some("localhost:8080")
        );
        assert_eq!(
            req.headers.get("connection").map(|s| s.as_str()),
            Some("keep-alive")
        );
    }

    #[test]
    fn test_headers_are_lowercase() {
        let raw = b"GET / HTTP/1.1\r\nContent-Type: text/html\r\nX-Custom-Header: value\r\n\r\n";
        let req = Request::parse(raw).expect("should parse");

        // Must be accessible in lowercase regardless of original casing
        assert!(req.headers.contains_key("content-type"));
        assert!(req.headers.contains_key("x-custom-header"));
    }
    #[test]
    fn test_content_length_parsed() {
        let raw = b"POST / HTTP/1.1\r\nContent-Length: 42\r\n\r\n";
        let req = Request::parse(raw).expect("should parse");

        assert_eq!(req.content_length(), 42);
    }
    #[test]
    fn test_content_length_defaults_to_zero() {
        let raw = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let req = Request::parse(raw).expect("should parse");

        assert_eq!(req.content_length(), 0);
    }
    #[test]
    fn test_header_value_with_colon() {
        // Header values can contain colons — must split on first colon only
        let raw = b"GET / HTTP/1.1\r\nHost: localhost:8080\r\n\r\n";
        let req = Request::parse(raw).expect("should parse");

        // Value should be "localhost:8080" not just "localhost"
        assert_eq!(
            req.headers.get("host").map(|s| s.as_str()),
            Some("localhost:8080")
        );
    }
    // ── Body ──────────────────────────────────────────────────────────────
    #[test]
    fn test_body_is_captured() {
        let raw = b"POST /upload HTTP/1.1\r\nHost: localhost\r\n\r\nbody content here";
        let req = Request::parse(raw).expect("should parse");

        assert_eq!(req.body, b"body content here");
    }
    #[test]
    fn test_empty_body_on_get() {
        let raw = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let req = Request::parse(raw).expect("should parse");

        assert!(req.body.is_empty());
    }
    #[test]
    fn test_binary_body() {
        // Body should survive as raw bytes even if not valid UTF-8
        let mut raw = b"POST /upload HTTP/1.1\r\nHost: localhost\r\n\r\n".to_vec();
        raw.extend_from_slice(&[0xFF, 0xFE, 0x00, 0x01]);
        let req = Request::parse(&raw).expect("should parse");

        assert_eq!(req.body, &[0xFF, 0xFE, 0x00, 0x01]);
    }
    // ── Malformed requests ────────────────────────────────────────────────

    #[test]
    fn test_empty_buffer_returns_none() {
        let raw = b"";
        assert!(Request::parse(raw).is_none());
    }

    #[test]
    fn test_missing_separator_returns_none() {
        // No \r\n\r\n — headers never end
        let raw = b"GET / HTTP/1.1\r\nHost: localhost";
        assert!(Request::parse(raw).is_none());
    }

    #[test]
    fn test_missing_path_returns_none() {
        // Only method, no path or version
        let raw = b"GET\r\n\r\n";
        assert!(Request::parse(raw).is_none());
    }

    #[test]
    fn test_completely_garbage_input_returns_none() {
        let raw = b"GARBAGE DATA WITH NO STRUCTURE";
        assert!(Request::parse(raw).is_none());
    }
}

//_______________ test chunks reading of body ... _______________________________________

#[test]
fn test_chunked_body_decoded() {
    // "5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"
    let chunked_body = b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
    let decoded = decode_chunked(chunked_body).unwrap();
    assert_eq!(decoded, b"hello world");
}

#[test]
fn test_chunked_request_parsed() {
    let raw = b"POST /upload HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n";
    let req = Request::parse(raw).expect("should parse");
    assert_eq!(req.body, b"hello");
}

#[test]
fn test_chunked_empty_body() {
    let raw =
        b"POST /upload HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n";
    let req = Request::parse(raw).expect("should parse");
    assert!(req.body.is_empty());
}

#[test]
fn test_malformed_chunk_size_returns_none() {
    // "gg" is not valid hex
    let raw = b"POST /upload HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\ngg\r\nhello\r\n0\r\n\r\n";
    assert!(Request::parse(raw).is_none());
}
