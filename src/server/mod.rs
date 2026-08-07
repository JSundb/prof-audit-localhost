pub mod default_html;
pub mod error;
mod event_loop;
mod handler;
mod router;
mod server;
mod server_socket;

pub use error::error_response_from_config;
pub use event_loop::run_loop;
pub use router::match_route;
pub use server::Server;
pub use server_socket::ServerSocket;
