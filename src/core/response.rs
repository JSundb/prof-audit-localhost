use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

#[derive(Debug)]
pub struct Response {
    pub status_code: u16,
    pub reason_phrase: String,
    pub headers: HashMap<String, String>,
    pub cookies: Vec<String>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status_code: u16, reason_phrase: &str) -> Self {
        Self {
            status_code,
            reason_phrase: reason_phrase.to_string(),
            headers: HashMap::new(),
            cookies: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        let body_bytes = body.into();
        self.headers
            .insert("Content-Length".to_string(), body_bytes.len().to_string());
        self.body = body_bytes;
        self
    }

    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }

    /// Set a cookie with optional attributes
    pub fn set_cookie(
        mut self,
        name: &str,
        value: &str,
        path: Option<&str>,
        max_age: Option<u64>,
        http_only: bool,
    ) -> Self {
        let mut cookie = format!("{}={}", name, value);
        if let Some(p) = path {
            cookie.push_str(&format!("; Path={}", p));
        }
        if let Some(age) = max_age {
            cookie.push_str(&format!("; Max-Age={}", age));
        }
        if http_only {
            cookie.push_str("; HttpOnly");
        }
        self.cookies.push(cookie);
        self
    }

    /// Serialize the full HTTP response into bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut response = String::new();

        // Status line
        let _ = write!(
            response,
            "HTTP/1.1 {} {}\r\n",
            self.status_code, self.reason_phrase
        );

        // Headers
        for (key, value) in &self.headers {
            let _ = write!(response, "{}: {}\r\n", key, value);
        }

        // Cookies
        for cookie in &self.cookies {
            let _ = write!(response, "Set-Cookie: {}\r\n", cookie);
        }

        // End of headers
        response.push_str("\r\n");

        let mut bytes = response.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }

    pub fn redirect(location: String, status_code: u16) -> Self {
        let reason_phrase = match status_code {
            301 => "Moved Permanently",
            302 => "Found",
            303 => "See Other",
            307 => "Temporary Redirect",
            308 => "Permanent Redirect",
            _ => "Redirect",
        }
        .to_string();

        let mut headers = HashMap::new();
        headers.insert("Location".to_string(), location.clone());
        headers.insert("Content-Type".to_string(), "text/html".to_string());

        let body = format!(
            "<html><body><h1>{} Redirect</h1><p>Redirecting to <a href=\"{}\">{}</a></p></body></html>",
            status_code, location, location
        );

        Self {
            status_code,
            reason_phrase,
            headers,
            cookies: Vec::new(),
            body: body.into_bytes(),
        }
    }
}
