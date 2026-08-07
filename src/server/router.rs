use crate::config::RouteConfig;
use std::collections::HashMap;

/// Nginx-style route matcher:
/// - Finds the **longest prefix** that matches the URI.
/// - Ensures prefix boundaries are respected ("/static" doesn’t match "/statics").
/// - Returns both the matched prefix and its RouteConfig.
pub fn match_route<'a>(
    routes: &'a HashMap<String, RouteConfig>,
    uri: &str,
) -> Option<(&'a str, &'a RouteConfig)> {
    let mut best_match: Option<(&str, &RouteConfig)> = None;
    let mut best_len = 0;

    for (prefix, cfg) in routes.iter() {
        // Must start with prefix, and match a boundary or be exact
        if uri == prefix || uri.starts_with(prefix) {
            // Ensure prefix boundary ("/static" shouldn't match "/static2")
            if uri == prefix
                || uri
                    .strip_prefix(prefix)
                    .map_or(false, |r| r.starts_with('/') || r.is_empty())
            {
                if prefix.len() > best_len {
                    best_len = prefix.len();
                    best_match = Some((prefix.as_str(), cfg));
                }
            }
        }
    }

    best_match
}
