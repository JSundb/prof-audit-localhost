use crate::config::ServerConfig;
use crate::core::{Request, Response};
use crate::server::error_response_from_config;
use std::path::Path;
use std::process::Command;

/// Very basic MIME type guessing based on file extension.
/// Extend as needed for your use case.
pub fn guess_mime_type(filename: &str) -> &str {
    if let Some(ext) = filename.rsplit('.').next() {
        match ext {
            "html" => "text/html",
            "htm" => "text/html",
            "css" => "text/css",
            "js" => "application/javascript",
            "json" => "application/json",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            "txt" => "text/plain",
            "ico" => "image/x-icon",
            "wasm" => "application/wasm",
            _ => "application/octet-stream", // unknown extension
        }
    } else {
        "application/octet-stream" // no extension
    }
}

pub fn resolve_cgi_interpreter(path: &Path, config: &ServerConfig) -> Option<String> {
    let extension = path.extension().and_then(|e| e.to_str())?;
    let ext_lowercase = extension.to_ascii_lowercase();
    let candidates = [
        format!(".{}", extension),
        extension.to_string(),
        format!(".{}", ext_lowercase),
        ext_lowercase,
    ];

    candidates
        .into_iter()
        .find_map(|ext| config.cgi_handlers.get(&ext).cloned())
}

pub fn set_cgi_env(
    cmd: &mut Command,
    script_path: &Path,
    request: &Request,
    config: &ServerConfig,
    local_port: u16,
) -> Result<(), Response> {
    let (uri_path, query) = split_uri(&request.uri);

    cmd.env("GATEWAY_INTERFACE", "CGI/1.1");
    cmd.env("REQUEST_METHOD", &request.method);
    cmd.env("QUERY_STRING", query);
    cmd.env("SERVER_PROTOCOL", &request._version);
    cmd.env("SCRIPT_NAME", uri_path);
    cmd.env("SCRIPT_FILENAME", script_path.as_os_str());

    // Validate Host/name/port and use the validated values
    let (server_name, host_port) = check_name_and_port(request, config, local_port)?;

    // Set validated server name and port
    cmd.env("SERVER_NAME", &server_name);
    cmd.env("SERVER_PORT", host_port.to_string());
    cmd.env("DOCUMENT_ROOT", &config.root);

    if let Some(ct) = request.headers.get("content-type") {
        cmd.env("CONTENT_TYPE", ct);
    }
    // Always reflect the parsed body length for CGI
    cmd.env("CONTENT_LENGTH", request.body.len().to_string());

    for (k, v) in &request.headers {
        let mut up = String::with_capacity(k.len() + 5);
        up.push_str("HTTP_");
        for ch in k.chars() {
            match ch {
                '-' => up.push('_'),
                c => up.push(c.to_ascii_uppercase()),
            }
        }
        cmd.env(up, v);
    }
    Ok(())
}

pub fn split_uri(uri: &str) -> (&str, &str) {
    if let Some((path, query)) = uri.split_once('?') {
        (path, query)
    } else {
        (uri, "")
    }
}

/// Find pattern in buffer
pub fn find_sequence(buffer: &[u8], pattern: &[u8]) -> Option<usize> {
    buffer.windows(pattern.len()).position(|w| w == pattern)
}

pub fn default_reason_phrase(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "OK",
    }
}

pub fn parse_cgi_output(output: &[u8], config: &ServerConfig) -> Response {
    // Potentially add redirect functionality and default content type
    let (header_bytes, body_bytes) = if let Some(pos) = find_sequence(output, b"\r\n\r\n") {
        (&output[..pos], &output[pos + 4..])
    } else if let Some(pos) = find_sequence(output, b"\n\n") {
        (&output[..pos], &output[pos + 2..])
    } else {
        let resp = Response::new(200, "OK");
        return resp.with_body(output.to_vec());
    };

    let header_text = String::from_utf8_lossy(header_bytes);
    let mut status_code = 200u16;
    let mut reason = String::from("OK");
    let mut headers: Vec<(String, String)> = Vec::new();

    for line in header_text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim();
            let value = value.trim();
            if name.eq_ignore_ascii_case("Status") {
                let mut parts = value.split_whitespace();
                if let Some(code_str) = parts.next() {
                    if let Ok(code) = code_str.parse::<u16>() {
                        status_code = code;
                        let rest = parts.collect::<Vec<_>>().join(" ");
                        if !rest.is_empty() {
                            reason = rest;
                        } else {
                            reason = default_reason_phrase(code).to_string();
                        }
                    }
                }
            } else {
                headers.push((name.to_string(), value.to_string()));
            }
        }
    }

    if status_code != 200u16 {
        error_response_from_config(status_code, config)
    } else {
        let mut resp = Response::new(status_code, &reason);
        for (key, value) in headers {
            if key.eq_ignore_ascii_case("Content-Length") {
                continue;
            }
            resp = resp.header(&key, &value);
        }
        resp.with_body(body_bytes.to_vec())
    }
}

pub fn check_name_and_port(
    request: &Request,
    config: &ServerConfig,
    local_port: u16,
) -> Result<(String, u16), Response> {
    // Check that host header exists
    let host = match request.headers.get("host") {
        Some(h) if !h.trim().is_empty() => h.trim(),
        _ => {
            return Err(error_response_from_config(400, config));
        }
    };

    // Parse server name and port from host header
    let (server_name, host_port) = match host.rsplit_once(':') {
        Some((name, port_str))
            if !name.is_empty() && port_str.chars().all(|c| c.is_ascii_digit()) =>
        {
            let p = match port_str.parse::<u16>() {
                Ok(v) => v,
                Err(_) => {
                    return Err(error_response_from_config(400, config));
                }
            };
            (name, p)
        }
        _ => {
            return Err(error_response_from_config(400, config));
        }
    };

    // Enforce server name if configured
    if let Some(cfg_name) = &config.server_name {
        if !server_name.eq_ignore_ascii_case(cfg_name) {
            return Err(error_response_from_config(400, config));
        }
    }

    // Enforce port matches socket's local port
    if host_port != local_port {
        return Err(error_response_from_config(400, config));
    }

    Ok((server_name.to_string(), host_port))
}
