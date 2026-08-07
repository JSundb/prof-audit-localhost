use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::config::{AdminConfig, ServerConfig};
use crate::core::{Request, Response};
use crate::server::error_response_from_config;

/// Represents the active administrator session and credentials.
#[derive(Debug, Clone)]
pub struct Admin {
    pub config: AdminConfig,
    pub session_id: Option<String>,
    pub session_created: Option<Instant>,
    pub session_timeout: Duration,
}

impl Admin {
    /// Create a new `Admin` with default credentials and timeout.
    pub fn new(config: AdminConfig) -> Self {
        Self {
            config,
            session_id: None,
            session_created: None,
            session_timeout: Duration::from_secs(3600), // 1 hour default
        }
    }

    /// Authenticate a username/password pair.
    /// Returns `true` and generates a session if successful.
    pub fn authenticate(&mut self, username: &str, password: &str) -> bool {
        if username == self.config.username && password == self.config.password {
            self.create_session();
            true
        } else {
            false
        }
    }

    /// Creates a new session and returns the generated session ID.
    pub fn create_session(&mut self) -> String {
        let session = Uuid::new_v4().to_string();
        self.session_id = Some(session.clone());
        self.session_created = Some(Instant::now());
        session
    }

    /// Validate an incoming request’s session cookie.
    pub fn validate_request(&self, request: &Request) -> bool {
        let cookie = request.get_cookie("session_id");
        self.validate_session_cookie(cookie)
    }

    /// Check if the provided cookie matches the current session.
    pub fn validate_session_cookie(&self, cookie: Option<String>) -> bool {
        match (&self.session_id, &self.session_created, cookie.as_deref()) {
            (Some(active_id), Some(created), Some(c)) if c == active_id => {
                created.elapsed() <= self.session_timeout
            }
            _ => false,
        }
    }

    /// Clears the session (e.g., on logout).
    pub fn _invalidate_session(&mut self) {
        self.session_id = None;
        self.session_created = None;
    }

    pub fn handle_login(&mut self, request: &Request, config: &ServerConfig) -> Response {
        let form = request.parse_form(); // parses POST form data
        let username = form.get("username").map(|s| s.as_str()).unwrap_or("");
        let password = form.get("password").map(|s| s.as_str()).unwrap_or("");

        if self.authenticate(username, password) {
            // Authentication successful; session already created
            let session_id = self.session_id.as_ref().unwrap().clone();

            return Response::redirect("/".to_string(), 302).set_cookie(
                "session_id",
                &session_id,
                Some("/"),
                None,
                true,
            );
        }

        // Invalid credentials → show login page
        error_response_from_config(401, config)
    }
}
