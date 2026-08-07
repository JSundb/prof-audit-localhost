use crate::config::ServerConfig;
use crate::core::Response;
use crate::server::handler::{default_reason_phrase, guess_mime_type};
use std::fs;
use std::path::Path;

/// Return an error response using a custom page if configured under `root/errors`.
pub fn error_response_from_config(status: u16, config: &ServerConfig) -> Response {
    if let Some(err_cfg) = config.errors.get(&status.to_string()) {
        if let Some(filename) = &err_cfg.filename {
            let path = Path::new(&config.root).join("errors").join(filename);
            match fs::read(&path) {
                Ok(bytes) => {
                    let reason = default_reason_phrase(status);
                    let filename = path.to_string_lossy();
                    let mime = guess_mime_type(&filename);
                    return Response::new(status, reason)
                        .header("Content-Type", mime)
                        .with_body(bytes);
                }
                Err(e) => {
                    eprintln!(
                        "Warning: failed to read custom error page '{}': {}",
                        path.display(),
                        e
                    );
                }
            }
        } else {
            eprintln!(
                "Warning: custom error {} configured without a filename; using default response",
                status
            );
        }
    }
    // Fallback if no default error page has been defined
    let reason = default_reason_phrase(status);
    let title = format!("{} {}", status, reason);
    let body = format!(
        "<!DOCTYPE html>\n<html>\n<head><meta charset=\"utf-8\"><title>{}</title></head>\n<body>\n  <h1>{}</h1>\n</body>\n</html>\n",
        title, title
    );
    Response::new(status, reason)
        .header("Content-Type", "text/html; charset=utf-8")
        .with_body(body)
}
