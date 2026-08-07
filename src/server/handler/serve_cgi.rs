use super::utils::{parse_cgi_output, resolve_cgi_interpreter, set_cgi_env};
use crate::config::ServerConfig;
use crate::core::{Request, Response};
use crate::server::error_response_from_config;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn serve_cgi_file(
    path: &Path,
    request: &Request,
    config: &ServerConfig,
    local_port: u16,
) -> Response {
    let interpreter = match resolve_cgi_interpreter(path, config) {
        Some(cmd) => cmd,
        None => {
            return error_response_from_config(502, config);
        }
    };

    // Remove duplicate root directory by resolving absolute path
    let abs_path = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            return error_response_from_config(404, config);
        }
    };

    // Prepare command: [interpreter, script_path]
    let mut cmd = Command::new(&interpreter);
    cmd.arg(&abs_path);

    // Working directory = script's directory
    if let Some(dir) = abs_path.parent() {
        cmd.current_dir(dir);
    }

    // Pipe stdin/stdout
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped());

    // Environment variables per CGI/1.1 (with strict Host/port validation)
    if let Err(resp) = set_cgi_env(&mut cmd, &abs_path, request, config, local_port) {
        return resp;
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => {
            return error_response_from_config(502, config);
        }
    };

    // Send request body to CGI stdin
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(_) = stdin.write_all(&request.body) {
            return error_response_from_config(502, config);
        }
        let _ = stdin.flush();
        drop(stdin);
    }

    // Read CGI stdout fully
    let mut out = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_end(&mut out);
    }

    let _ = child.wait();

    // Parse CGI headers/body
    parse_cgi_output(&out, config)
}
