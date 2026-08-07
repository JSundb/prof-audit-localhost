use crate::config::ServerConfig;
use crate::core::Response;
use crate::server::error_response_from_config;

use super::utils::guess_mime_type;
use std::fs;
use std::path::Path;

pub fn serve_static_file(path: &Path, config: &ServerConfig) -> Response {
    match fs::read(path) {
        Ok(contents) => {
            let filename = path.to_string_lossy();
            let mime = guess_mime_type(&filename);
            Response::new(200, "OK")
                .header("Content-Type", mime)
                .with_body(contents)
        }
        Err(_) => error_response_from_config(404, config),
    }
}
