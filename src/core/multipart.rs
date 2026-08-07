use crate::core::Request;
use std::collections::HashMap;

// Find the position of a subslice within another
pub fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Extracts the boundary string from a multipart/form-data Content-Type header
pub fn extract_boundary(request: &Request) -> Option<String> {
    let content_type = request.headers.get("content-type")?;
    if !content_type.starts_with("multipart/form-data") {
        return None;
    }
    content_type
        .split("boundary=")
        .nth(1)
        .map(|b| b.trim().to_string())
}

/// Extracts `filename="..."` from Content-Disposition header
pub fn extract_filename(part: &str) -> Option<String> {
    let disposition_line = part.lines().find(|l| l.contains("Content-Disposition"))?;
    disposition_line.split(';').find_map(|segment| {
        let segment = segment.trim();
        if segment.starts_with("filename=") {
            Some(
                segment
                    .trim_start_matches("filename=")
                    .trim_matches('"')
                    .to_string(),
            )
        } else {
            None
        }
    })
}

#[derive(Debug)]
pub struct MultipartPart {
    pub _headers: HashMap<String, String>,
    pub filename: Option<String>,
    pub content: Vec<u8>,
}

pub fn parse_multipart(body: &[u8], boundary: &str) -> Vec<MultipartPart> {
    let boundary_marker = format!("--{}", boundary);
    let boundary_bytes = boundary_marker.as_bytes();

    let mut parts = Vec::new();
    let mut start = 0;
    while let Some(pos) = find_subslice(&body[start..], boundary_bytes) {
        let part = &body[start..start + pos];
        if !part.is_empty() {
            if let Some(parsed) = parse_part(part) {
                parts.push(parsed);
            }
        }
        start += pos + boundary_bytes.len();
    }
    parts
}

/// Parses an individual part (headers + content)
fn parse_part(raw: &[u8]) -> Option<MultipartPart> {
    let mut headers = HashMap::new();
    let content_start = find_subslice(raw, b"\r\n\r\n")?;
    let header_str = String::from_utf8_lossy(&raw[..content_start]);

    for line in header_str.lines() {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_string(), value.trim().to_string());
        }
    }

    let filename = extract_filename(&header_str);
    let mut content = raw[content_start + 4..].to_vec();

    // Trim trailing CRLF if present
    if content.ends_with(b"\r\n") {
        content.truncate(content.len() - 2);
    }

    Some(MultipartPart {
        _headers: headers,
        filename,
        content,
    })
}
