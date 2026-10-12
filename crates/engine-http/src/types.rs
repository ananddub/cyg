//! # Core HTTP Types
//!
//! Protocol-agnostic request/response values. Backend implementations
//! (`HttpBackend`) parse the wire format into these types and serialize
//! responses back out. Game/endpoint code only ever sees these.

use std::collections::HashMap;

/// HTTP methods served by the framework.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl Method {
    pub fn from_str_token(token: &str) -> Option<Self> {
        match token.to_ascii_uppercase().as_str() {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "PATCH" => Some(Self::Patch),
            "DELETE" => Some(Self::Delete),
            "HEAD" => Some(Self::Head),
            "OPTIONS" => Some(Self::Options),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

/// Case-insensitive HTTP header collection preserving insertion order.
#[derive(Debug, Clone, Default)]
pub struct Headers(Vec<(String, String)>);

impl Headers {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.0.push((name.into(), value.into()));
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        let lower = name.to_ascii_lowercase();
        self.0
            .iter()
            .find(|(k, _)| k.to_ascii_lowercase() == lower)
            .map(|(_, v)| v.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

/// A fully parsed HTTP request, independent of the underlying wire protocol.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: Method,
    /// Request target as received, e.g. `/rooms/7/status?verbose=1`.
    pub target: String,
    /// Path portion with the query string stripped.
    pub path: String,
    /// Decoded query parameters.
    pub query: HashMap<String, String>,
    pub headers: Headers,
    pub body: Vec<u8>,
    /// Path parameters captured by the router (`:name` segments).
    pub params: HashMap<String, String>,
}

impl Request {
    pub fn query_param(&self, name: &str) -> Option<&str> {
        self.query.get(name).map(|s| s.as_str())
    }

    pub fn param(&self, name: &str) -> Option<&str> {
        self.params.get(name).map(|s| s.as_str())
    }

    pub fn body_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.body).ok()
    }

    pub fn content_type(&self) -> Option<&str> {
        self.headers.get("content-type")
    }
}

/// An HTTP response produced by endpoint handlers.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: Headers,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: Headers::new(),
            body: Vec::new(),
        }
    }

    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name, value);
        self
    }

    pub fn text(body: impl Into<String>) -> Self {
        let body = body.into();
        Self::new(200)
            .with_header("content-type", "text/plain; charset=utf-8")
            .with_body(body)
    }

    pub fn json(body: impl Into<String>) -> Self {
        let body = body.into();
        Self::new(200)
            .with_header("content-type", "application/json")
            .with_body(body)
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::error(400, msg)
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::error(404, msg)
    }

    pub fn error(status: u16, msg: impl Into<String>) -> Self {
        let body = format!(r#"{{"error":"{}"}}"#, escape_json(msg.into().as_str()));
        Self::new(status)
            .with_header("content-type", "application/json")
            .with_body(body)
    }

    pub fn status_text(status: u16) -> &'static str {
        match status {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            400 => "Bad Request",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            408 => "Request Timeout",
            411 => "Length Required",
            413 => "Content Too Large",
            431 => "Request Header Fields Too Large",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            503 => "Service Unavailable",
            _ => "Unknown Status",
        }
    }
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Escapes a string for embedding in a JSON document body built by hand.
pub fn json_escape(s: &str) -> String {
    escape_json(s)
}
