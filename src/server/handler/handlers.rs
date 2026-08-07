use super::serve_cgi::serve_cgi_file;
use super::serve_static::serve_static_file;
use super::utils::resolve_cgi_interpreter;
use crate::config::ServerConfig;
use crate::core::{Request, Response};
use std::path::Path;

/// Determines whether to serve a static file or invoke CGI for the given path, then execute the handler
pub fn execute_handler(
    path: &Path,
    request: &Request,
    config: &ServerConfig,
    local_port: u16,
) -> Response {
    // If the file extension matches any of the config.cgi_handlers keys, use cgi handling
    if resolve_cgi_interpreter(path, config).is_some() {
        serve_cgi_file(path, request, config, local_port)
    } else {
        serve_static_file(path, config)
    }
}
