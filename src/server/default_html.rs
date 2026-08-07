use crate::core::Response;
use crate::server::handler::default_reason_phrase;
use crate::server::ServerSocket;

pub fn default_index_response(sockets: &Vec<ServerSocket>) -> Response {
    let mut body = String::from(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Registered Servers & Routes</title>
</head>
<body>
    <h1>Registered Servers & Routes</h1>
"#,
    );

    for socket in sockets {
        for config in &socket.configs {
            let server_name = config.server_name.as_deref().unwrap_or("(no name)");

            body.push_str(&format!(
                "<h2>Server: {} on {}:{}</h2>\n<ul>\n",
                server_name,
                socket.addr.ip(),
                socket.addr.port()
            ));

            for (route, cfg) in &config.routes {
                let hostname = if let Some(name) = &config.server_name {
                    if !name.is_empty() {
                        name.clone()
                    } else {
                        socket.addr.ip().to_string()
                    }
                } else {
                    socket.addr.ip().to_string()
                };

                let methods = if let Some(methods) = &cfg.methods {
                    methods.join(", ")
                } else {
                    "ALL".to_string()
                };

                body.push_str(&format!(
                    r#"  <li><span class="route-methods">[{methods}]</span> <a href="http://{}:{}{}">{}</a></li>"#,
                    hostname,
                    socket.addr.port(),
                    route,
                    route,
                    methods = methods
                ));
                body.push('\n');
            }

            body.push_str("</ul>\n");
        }
    }

    body.push_str("</body>\n</html>");

    Response::new(200, default_reason_phrase(200))
        .header("Content-Type", "text/html; charset=utf-8")
        .with_body(body)
}
