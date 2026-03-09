// src/request.rs

#[derive(Debug)]
pub enum Method {
    Get,
    Post,
    Delete,
    Unknown(String),
}

#[derive(Debug)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub version: String,
}

impl Request {
    pub fn parse(buffer: &[u8]) -> Option<Request> {
        let raw = String::from_utf8_lossy(buffer);
        let first_line = raw.lines().next()?;
        let mut parts = first_line.split_whitespace();

        let method = match parts.next()? {
            "GET" => Method::Get,
            "POST" => Method::Post,
            "DELETE" => Method::Delete,
            other => Method::Unknown(other.to_string()),
        };

        let path = parts.next()?.to_string();
        let version = parts.next()?.to_string();

        Some(Request {
            method,
            path,
            version,
        })
    }
}
