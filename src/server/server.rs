use std::collections::HashMap;
use std::io::{self};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::{RouteConfig, ServerConfig};
use crate::core::{Request, Response};
use crate::server::default_html::default_index_response;
use crate::server::error_response_from_config;
use crate::server::handler::{
    execute_handler,
    generate_directory_listing,
    resolve_cgi_interpreter,
    resolve_target_path,
    serve_static_file,
    Admin,
};
use crate::server::match_route;
use crate::server::run_loop;
use crate::server::ServerSocket;
use crate::ClientConnection;
use crate::Config;

#[derive(Debug)]
pub struct Server {
    pub sockets: Vec<ServerSocket>,
    pub clients: Vec<ClientConnection>,
    pub client_timeout: Duration,
    pub admin: Admin,
}

impl Server {
    pub fn from_config(config: &Config) -> std::io::Result<Self> {
        let mut grouped: HashMap<SocketAddr, Vec<ServerConfig>> = HashMap::new();

        // Group configs by SocketAddr
        for server in &config.servers {
            for &port in &server.ports {
                let addr_str = format!("{}:{}", server.server_address, port);
                match addr_str.parse::<SocketAddr>() {
                    Ok(addr) => {
                        grouped.entry(addr).or_default().push(server.clone());
                    }
                    Err(e) => {
                        eprintln!("[!] Invalid address '{}': {}", addr_str, e);
                    }
                }
            }
        }

        let client_timeout = Duration::from_secs(config.client_timeout_secs);
        let mut sockets = Vec::new();

        for (addr, configs) in grouped {
            match ServerSocket::try_bind(addr, configs) {
                Ok(socket) => sockets.push(socket),
                Err(e) => eprintln!("[!] Failed to bind {}: {}", addr, e),
            }
        }

        if sockets.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "No sockets bound",
            ));
        }

        let admin = Admin::new(config.admin.clone());

        Ok(Server {
            sockets,
            clients: Vec::new(),
            client_timeout,
            admin,
        })
    }

    pub fn handle_directory_request(
        &self,
        root_dir: &Path,
        route_cfg: &RouteConfig,
        request_subpath: &str,
        route_prefix: &str,
        request: &Request,
        config: &ServerConfig,
        client: &ClientConnection,
    ) -> Response {
        // Full target path is base_path + request_subpath
        let target_path = root_dir.join(request_subpath.trim_start_matches('/'));
        let Ok(target_path) = target_path.canonicalize() else {
            return error_response_from_config(404, config);
        };
        let Ok(root_dir) = root_dir.canonicalize() else {
            return error_response_from_config(500, config);
        };
        if !target_path.starts_with(&root_dir) {
            return error_response_from_config(403, config);
        }

        if target_path.is_file() {
            return execute_handler(&target_path, request, config, client.local_addr.port());
        }

        if target_path.is_dir() {
            if route_cfg.directory_listing {
                return generate_directory_listing(
                    &target_path,
                    route_prefix,
                    route_cfg.upload_dir.is_some(),
                );
            }

            if let Some(file) = route_cfg.filename.clone() {
                let file_path = target_path.join(file);
                return execute_handler(&file_path, request, config, client.local_addr.port());
            }

            let index_path = target_path.join("index.html");
            if index_path.exists() {
                return execute_handler(&index_path, request, config, client.local_addr.port());
            }
            return error_response_from_config(403, config);
        }

        error_response_from_config(404, config)
    }

    fn handle_request(&mut self, request: &Request, client: &ClientConnection) -> Response {
        // Step 1: Identify which socket the client connected to
        let socket = match self.sockets.iter().find(|s| s.addr == client.local_addr) {
            Some(sock) => sock,
            None => {
                eprintln!("[!] No socket found for local addr {}", client.local_addr);
                let fallback_config = self
                    .sockets
                    .iter()
                    .find_map(|sock| sock.configs.first())
                    .expect("Server must have at least one associated config");
                return error_response_from_config(500, fallback_config);
            }
        };

        // Step 2: Resolve configuration based on Host header
        let host_header = request.headers.get("host").map(|s| s.as_str());
        let config = socket.resolve_config(host_header);
        let root_dir = Path::new(&config.root);

        // Step 2.5: Check admin access requirement
        if config.admin_access {
            let is_authenticated = self.admin.validate_request(request);

            // If not authenticated
            if !is_authenticated {
                if request.uri == "/login" {
                    if request.method.eq_ignore_ascii_case("POST") {
                        // POST → attempt login
                        return self.admin.handle_login(request, config);
                    } else {
                        // GET → serve login page
                        return serve_static_file(&root_dir.join("login.html"), config);
                    }
                } else {
                    // Any other admin route redirect to login
                    return Response::redirect("/login".to_string(), 302);
                }
            }
        }

        // Step 3: Find route match
        let (route_prefix, route_cfg) = match match_route(&config.routes, &request.uri) {
            Some(r) => r,
            None => return error_response_from_config(404, config),
        };

        // Step 4: Redirect
        if let Some(redirect) = &route_cfg.redirect {
            return Response::redirect(redirect.to.clone(), redirect.code);
        }

        // Step 5: Enforce allowed methods
        if let Err(_) = route_cfg.check_method(&request.method) {
            return error_response_from_config(405, config);
        }

        // Step 6: Upload handling (POST → upload_dir)
        if let Some(upload_dir) = &route_cfg.upload_dir {
            let full_target_path =
                resolve_target_path(&request.uri, &route_prefix, root_dir, &upload_dir);

            if request.method.eq_ignore_ascii_case("POST") {
                return Server::handle_upload(request, &full_target_path, config);
            }

            if request.method.eq_ignore_ascii_case("DELETE") {
                return Server::handle_delete(&full_target_path, config);
            }
        }

        // Step 7: Directory handling (GET/HEAD → directory or listing)
        if let Some(dir) = &route_cfg.directory {
            if !matches!(request.method.as_str(), "GET" | "HEAD") {
                return error_response_from_config(405, config);
            }
            let base_dir = root_dir.join(dir);
            let sub_path = &request.uri[route_prefix.len()..];
            let sub_path = if sub_path.is_empty() { "/" } else { sub_path };
            
            let full_path = base_dir.join(sub_path.trim_start_matches('/'));
            if full_path.is_dir() && !request.uri.ends_with('/') {
                let location = format!("{}/", request.uri);
                return Response::new(301, "Moved Permanently")
                    .header("Location", &location);
            }

            let route_prefix = format!("{}/{}", route_prefix.trim_end_matches('/'), sub_path.trim_start_matches('/'));

            return self.handle_directory_request(
                &base_dir,
                route_cfg,
                sub_path,
                &route_prefix,
                request,
                config,
                client,
            );
        }

        // Step 8: Static file handling (GET/HEAD filename)
        if let Some(filename) = &route_cfg.filename {
            let full_path = root_dir.join(filename);

            // Only enforce for non-cgi files
            if resolve_cgi_interpreter(&full_path, config).is_none() {
                if !matches!(request.method.as_str(), "GET" | "HEAD") {
                    return error_response_from_config(405, config);
                }
            }

            let full_path = root_dir.join(filename);
            return execute_handler(&full_path, request, config, client.local_addr.port());
        }

        // Step 9: Misconfiguration. serve default index
        default_index_response(&self.sockets)
    }

    pub fn handle_client_read(&mut self, client: &mut ClientConnection) -> io::Result<bool> {
        match client.read_into_buffer() {
            Ok(0) => {
                return Ok(false); // connection closed
            }

            Ok(_) => {
                if let Some(request) = client.parse_request() {
                    client.should_close = request
                        .headers
                        .get("connection")
                        .map(|v| v.eq_ignore_ascii_case("close"))
                        .unwrap_or(false);

                    let response = self.handle_request(&request, &client);
                    client.queue_response(&response.to_bytes()); // put into write_buffer
                }
                Ok(true) // keep connection open
            }

            // Keep client when no data is available instead of kicking them out by giving that error Ok(0)
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                Ok(true)
            }

            Err(e) => {
                eprintln!("[!] Error reading from {}: {}", client.peer_addr, e);
                let _ = client.stream.shutdown(std::net::Shutdown::Both);
                Ok(false)
            }
        }
    }

    pub fn handle_client_write(&mut self, client: &mut ClientConnection) -> io::Result<bool> {
        match client.flush_write_buffer() {
            Ok(true) => {
                Ok(true)
            }
            Ok(false) => {
                println!("[*] Client {} closed write connection", client.peer_addr);
                Ok(false)
            }
            Err(e) => {
                eprintln!("[!] Error writing to {}: {}", client.peer_addr, e);
                let _ = client.stream.shutdown(std::net::Shutdown::Both);
                Ok(false)
            }
        }
    }

    fn handle_upload(
        request: &Request,
        upload_directory: &PathBuf,
        config: &ServerConfig,
    ) -> Response {
        // Min and max upload size limits (in bytes)
        const MIN_UPLOAD_SIZE: usize = 1; // 1 byte minimum
        const MAX_UPLOAD_SIZE: usize = 1 * 1024 * 1024; // 10 MB maximum

        if let Err(_) = std::fs::create_dir_all(upload_directory) {
            return error_response_from_config(500, config);
        }

        if !request.is_multipart() {
            return error_response_from_config(400, config);
        }

        let parts = match request.multipart_parts() {
            Some(p) => p,
            None => return error_response_from_config(400, config),
        };

        for part in parts {
            // Skip empty uploads
            let filename = match &part.filename {
                Some(f) if !f.is_empty() => f,
                _ => continue, // no file selected, skip this part
            };

            let file_size = part.content.len();

            if file_size < MIN_UPLOAD_SIZE {
                return Response::new(400, "Bad Request")
                        .with_body("Upload too small. Please provide a larger file.")
            }

            if file_size > MAX_UPLOAD_SIZE {
                return Response::new(413, "Payload Too Large")
                        .with_body("Upload too large. Maximum allowed size exceeded.")
            }

            // Build full path under upload_directory
            let full_path = Path::new(upload_directory).join(filename);

            if let Some(parent) = full_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    eprintln!(
                        "Failed to create directories for {}: {}",
                        full_path.display(),
                        e
                    );
                    continue;
                }
            }

            match std::fs::write(&full_path, &part.content) {
                Ok(_) => println!("Saved file: {}", full_path.display()),
                Err(e) => eprintln!("Failed to save {}: {}", full_path.display(), e),
            }
        }

        Response::new(200, "OK").with_body("File uploaded successfully\n")
    }

    pub fn handle_delete(target_path: &Path, config: &ServerConfig) -> Response {
        if !target_path.exists() {
            return error_response_from_config(404, config);
        }

        let result = if target_path.is_file() {
            std::fs::remove_file(target_path)
        } else if target_path.is_dir() {
            std::fs::remove_dir_all(target_path)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Unknown file type",
            ))
        };

        match result {
            Ok(_) => Response::new(200, "OK")
                .with_body(format!("Deleted successfully: {}", target_path.display())),
            Err(_) => error_response_from_config(500, config),
        }
    }

    pub fn run(&mut self) {
        run_loop(self);
    }
}
