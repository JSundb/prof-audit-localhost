use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::{fs, path::Path};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub servers: Vec<ServerConfig>,

    #[serde(default = "default_timeout_secs")]
    pub client_timeout_secs: u64,

    #[serde(default)]
    pub admin: AdminConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub server_address: String,
    pub ports: Vec<u16>,
    pub server_name: Option<String>,

    #[serde(default)]
    pub root: String,

    #[serde(default = "default_client_max_body_size")]
    pub client_max_body_size: usize,

    #[serde(default)]
    pub routes: HashMap<String, RouteConfig>,

    /// Map file extensions to CGI interpreter/command
    #[serde(default)]
    pub cgi_handlers: HashMap<String, String>,

    /// HTTP status code -> custom error file
    #[serde(default)]
    pub errors: HashMap<String, RouteConfig>,

    #[serde(default)]
    pub admin_access: bool,
}

fn default_client_max_body_size() -> usize {
    10 * 1000 * 1000 // 10MB Default
}

fn default_timeout_secs() -> u64 {
    30 // default 30 seconds if not specified
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        let mut seen_servers: HashSet<(String, u16, String)> = HashSet::new();

        for server in &self.servers {
            if server.ports.is_empty() {
                return Err(format!("Server at {} has no ports", server.server_address));
            }

            if server.root.trim().is_empty() {
                return Err(format!(
                    "Server at {} must have a non-empty 'root' directory defined",
                    server.server_address
                ));
            }

            if !Path::new(&server.root).is_dir() {
                return Err(format!("Root directory '{}' does not exist", server.root));
            }

            for &port in &server.ports {
                // Empty string for nameless server
                let name = server.server_name.clone().unwrap_or_default();

                let key = (server.server_address.clone(), port, name.clone());

                if !seen_servers.insert(key) {
                    if name.is_empty() {
                        return Err(format!(
                            "Duplicate server name '{}' configured on {}:{}",
                            name, server.server_address, port
                        ));
                    } else {
                        return Err(format!(
                            "Duplicate server name '{}' configured on {}:{}",
                            name, server.server_address, port
                        ));
                    }
                }
            }

            for (route, cfg) in &server.routes {
                if !route.starts_with("/") {
                    eprintln!("Warning: route '{}' should start with '/'", route);
                }

                // A valid route must define at least one of these
                if cfg.filename.is_none()
                    && cfg.directory.is_none()
                    && cfg.redirect.is_none()
                    && cfg.upload_dir.is_none()
                {
                    eprintln!(
                        "Warning: Route '{}' has no directory, redirect, upload_dir, or filename defined. Default index will be served.",
                        route
                    );
                }

                // Check file existence
                if let Some(filename) = &cfg.filename {
                    let full_path = Path::new(&server.root).join(filename);
                    if !full_path.exists() {
                        eprintln!(
                            "Warning: route '{}' points to missing file: {}",
                            route,
                            full_path.display()
                        );
                    }
                }

                // Check directory existence
                if let Some(directory) = &cfg.directory {
                    if route == "/" {
                        return Err("Route '/' cannot serve a directory — use a subpath like '/files' instead.".to_string());
                    }

                    let full_path = Path::new(&server.root).join(directory);
                    if !full_path.exists() || !full_path.is_dir() {
                        eprintln!(
                            "Warning: route '{}' points to missing or invalid directory: {}",
                            route,
                            full_path.display()
                        );
                    }
                }

                // Validate upload dir (we create it later if needed)
                if let Some(upload_dir) = &cfg.upload_dir {
                    let path = Path::new(upload_dir);
                    if path.exists() && !path.is_dir() {
                        return Err(format!(
                            "Route '{}' defines an upload_dir that exists but is not a directory: {}",
                            route,
                            path.display()
                        ));
                    }
                }
            }

            // Validate custom error files under root/errors
            if !server.errors.is_empty() {
                let errors_dir = std::path::Path::new(&server.root).join("errors");
                for (code, cfg) in &server.errors {
                    // best-effort code parse to notify users early
                    if code.parse::<u16>().is_err() {
                        eprintln!("Warning: error code '{}' is not a valid u16", code);
                    }

                    let Some(filename) = &cfg.filename else {
                        eprintln!("Warning: custom error {} has no filename configured", code);
                        continue;
                    };

                    let full_path = errors_dir.join(filename);
                    if !full_path.exists() {
                        eprintln!(
                            "Warning: custom error {} file not found: {}",
                            code,
                            full_path.display()
                        );
                    }
                }
            }
        }

        Ok(())
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("Failed to read config: {}", e))?;

        let config: Config =
            toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {}", e))?;

        config.validate()?;

        Ok(config)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct RouteConfig {
    #[serde(default)]
    pub filename: Option<String>, // for single file

    #[serde(default)]
    pub directory: Option<String>, // for directory mapping

    #[serde(default)]
    pub directory_listing: bool, // default to false

    #[serde(default)]
    pub methods: Option<Vec<String>>, // allowed methods

    #[serde(default)]
    pub redirect: Option<RedirectConfig>, // optional redirect

    #[serde(default)]
    pub upload_dir: Option<String>,
}

impl RouteConfig {
    pub fn check_method(&self, method: &str) -> Result<(), String> {
        if let Some(allowed) = &self.methods {
            if !allowed.iter().any(|m| m.eq_ignore_ascii_case(method)) {
                return Err(allowed.join(", "));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedirectConfig {
    pub to: String, // Target URL or path

    #[serde(default = "default_redirect_code")]
    pub code: u16, // e.g., 301 or 302
}

fn default_redirect_code() -> u16 {
    302 // Default to 302 Found
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct AdminConfig {
    #[serde(default = "default_admin_username")]
    pub username: String,

    #[serde(default = "default_admin_password")]
    pub password: String,
}

fn default_admin_username() -> String {
    "admin".to_string()
}

fn default_admin_password() -> String {
    "password123".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Default, valid configuration for tests to individually malform differently
    fn valid_server_config() -> ServerConfig {
        ServerConfig {
            server_address: "127.0.0.1".to_string(),
            ports: vec![8081],
            server_name: Some("localhost".to_string()),
            root: ".".to_string(),
            routes: HashMap::new(),
            client_max_body_size: 10 * 1000 * 1000, // 10MB Default
            cgi_handlers: HashMap::new(),
            errors: HashMap::new(),
            admin_access: false,
        }
    }

    fn config_with_servers(servers: Vec<ServerConfig>) -> Config {
        Config {
            servers,
            client_timeout_secs: 30,
            admin: AdminConfig::default(),
        }
    }

    fn configure_route_with_methods(methods: &[&str]) -> RouteConfig {
        RouteConfig {
            filename: None,
            directory: None,
            directory_listing: false,
            methods: Some(methods.iter().map(|method| method.to_string()).collect()),
            redirect: None,
            upload_dir: None,
        }
    }

    #[test]
    fn allows_only_configured_methods() {
        let route = configure_route_with_methods(&["GET"]);
        assert!(route.check_method("GET").is_ok());
        assert!(route.check_method("POST").is_err());
        assert!(route.check_method("DELETE").is_err());

        let route = configure_route_with_methods(&["POST"]);
        assert!(route.check_method("GET").is_err());
        assert!(route.check_method("POST").is_ok());
        assert!(route.check_method("DELETE").is_err());

        let route = configure_route_with_methods(&["DELETE"]);
        assert!(route.check_method("GET").is_err());
        assert!(route.check_method("POST").is_err());
        assert!(route.check_method("DELETE").is_ok());

        let route = configure_route_with_methods(&["GET", "POST"]);
        assert!(route.check_method("GET").is_ok());
        assert!(route.check_method("POST").is_ok());
        assert!(route.check_method("DELETE").is_err());

        let route = configure_route_with_methods(&["GET", "DELETE"]);
        assert!(route.check_method("GET").is_ok());
        assert!(route.check_method("POST").is_err());
        assert!(route.check_method("DELETE").is_ok());

        let route = configure_route_with_methods(&["POST", "DELETE"]);
        assert!(route.check_method("GET").is_err());
        assert!(route.check_method("POST").is_ok());
        assert!(route.check_method("DELETE").is_ok());

        let route = configure_route_with_methods(&["GET", "POST", "DELETE"]);
        assert!(route.check_method("GET").is_ok());
        assert!(route.check_method("POST").is_ok());
        assert!(route.check_method("DELETE").is_ok());
    }

    #[test]
    fn rejects_server_with_no_ports() {
        let mut server = valid_server_config();
        server.ports = vec![];

        let config = config_with_servers(vec![server]);

        let error = config.validate().unwrap_err();
        assert!(error.contains("has no ports"));
    }

    #[test]
    fn rejects_server_with_empty_root() {
        let mut server = valid_server_config();
        server.root = "".to_string();

        let config = config_with_servers(vec![server]);

        let error = config.validate().unwrap_err();
        assert!(error.contains("non-empty"));
    }

    #[test]
    fn rejects_server_with_missing_root_directory() {
        let mut server = valid_server_config();
        server.root = "./directory_that_should_not_exist_123456789".to_string();

        let config = config_with_servers(vec![server]);

        let error = config.validate().unwrap_err();
        assert!(error.contains("does not exist"));
    }

    #[test]
    fn rejects_duplicate_server_name_on_same_address_and_port() {
        let server_one = valid_server_config();
        let server_two = valid_server_config();

        let config = config_with_servers(vec![server_one, server_two]);

        let error = config.validate().unwrap_err();
        assert!(error.contains("Duplicate server name"));
    }

    #[test]
    fn allows_same_server_name_and_port_on_different_addresses() {
        let server_one = valid_server_config();

        let mut server_two = valid_server_config();
        server_two.server_address = "127.0.0.2".to_string();

        let config = config_with_servers(vec![server_one, server_two]);

        assert!(config.validate().is_ok());
    }

    #[test]
    fn uses_default_client_max_body_size_when_not_configured() {
        // From string to make sure the construction itself, instead of the previously written helper, configures the right max body size
        let config: Config = toml::from_str(
            r#"
            [[servers]]
            server_address = "127.0.0.1"
            ports = [8080]
            server_name = "localhost"
            root = "."
            "#,
        )
        .unwrap();

        assert_eq!(config.servers[0].client_max_body_size, 10_000_000);
    }

    #[test]
    fn uses_configured_client_max_body_size() {
        let config: Config = toml::from_str(
            r#"
            [[servers]]
            server_address = "127.0.0.1"
            ports = [8080]
            server_name = "localhost"
            root = "."
            client_max_body_size = 123456
            "#,
        )
        .unwrap();

        assert_eq!(config.servers[0].client_max_body_size, 123456);
    }
}
