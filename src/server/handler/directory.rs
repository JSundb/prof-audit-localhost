use std::{fs, path::Path, path::PathBuf};

use crate::core::url_encode;
use crate::core::Response;

pub fn generate_directory_listing(
    dir: &Path,
    base_url_path: &str,
    upload_allowed: bool,
) -> Response {
    let mut html = String::from(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Directory Listing</title>
</head>
<body>
<h1>Directory Listing</h1>
"#,
    );

    if upload_allowed {
        // 🆕 Upload form that posts back to the same directory path
        html.push_str(&format!(
            r#"
<form class="upload-form" action="{}" method="POST" enctype="multipart/form-data">
  <label>Upload files:</label>
  <input type="file" name="files" id="files" multiple><br><br>

  <label>Or upload a folder:</label>
  <input type="file" name="folders" id="folders" webkitdirectory directory><br><br>

  <button type="submit">Upload</button>
</form>
"#,
            base_url_path
        ));
    }

    html.push_str("<ul>");

    // Add parent directory link if not at root
    if let Some(_) = dir.parent() {
        // Remove the trailing slash if it exists
        let trimmed = base_url_path.trim_end_matches('/');

        // For example, if base_url_path = "/files/subdir", parent is "/files"
        let parent_path = if let Some(pos) = trimmed.rfind('/') {
            &base_url_path[..pos]
        } else {
            "/"
        };

        html.push_str(&format!(
            r#"<li class="parent"><a href="{}">📁 ..</a></li>"#,
            parent_path
        ));
    }

    if let Ok(entries) = fs::read_dir(dir) {
        let mut items: Vec<(String, bool)> = entries
            .filter_map(|e| e.ok())
            .map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let is_dir = entry.path().is_dir();
                (name, is_dir)
            })
            .collect();

        items.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

        for (name, is_dir) in items {
            let encoded_name = url_encode(&name);
            // Prepend base_url_path, making sure there's exactly one slash between
            let url = if base_url_path.ends_with('/') {
                format!("{}{}", base_url_path, encoded_name)
            } else {
                format!("{}/{}", base_url_path, encoded_name)
            };

            let icon = if is_dir { "📁" } else { "📄" };

            // Each item has a delete button with a data-filename attribute
            html.push_str(&format!(
                r#"<li>
    <button class="delete-btn" data-filename="{encoded_name}">🗑️</button>
    <a href="{url}">{icon} {name}</a>
</li>"#,
                url = url,
                icon = icon,
                name = name,
                encoded_name = encoded_name
            ));
        }

        // Add JS for delete buttons
        html.push_str(
            r#"
<script>
document.querySelectorAll('.delete-btn').forEach(btn => {
    btn.addEventListener('click', async (e) => {
        e.preventDefault();
        const filename = btn.dataset.filename;
        if (!confirm(`Delete ${filename}?`)) return;

        const url = window.location.pathname.replace(/\/$/, '') + '/' + filename;
        try {
            const resp = await fetch(url, { method: 'DELETE' });
            if (resp.ok) {
                // Reload directory listing
                window.location.reload();
            } else {
                alert('Failed to delete ' + filename);
            }
        } catch (err) {
            alert('Error deleting file: ' + err);
        }
    });
});

document.querySelectorAll('.upload-form').forEach(form => {
    form.addEventListener('submit', async (e) => {
        e.preventDefault();

        const formData = new FormData(form);
        try {
            const resp = await fetch(form.action, {
                method: 'POST',
                body: formData
            });

            if (resp.ok) {
                // Reload directory listing after successful upload
                window.location.reload();
            } else {
                const text = await resp.text();
                alert('Upload failed: ' + text);
            }
        } catch (err) {
            alert('Error uploading: ' + err);
        }
    });
});
</script>
"#,
        );
    } else {
        html.push_str("<li><em>Could not read directory</em></li>");
    }

    html.push_str("</ul></body></html>");

    Response::new(200, "OK")
        .header("Content-Type", "text/html")
        .with_body(html)
}

pub fn resolve_target_path(
    request_uri: &str,
    route_prefix: &str,
    root_dir: &Path,
    upload_dir: &str,
) -> PathBuf {
    // Compute the relative subpath under the route
    let sub_path = request_uri.strip_prefix(&route_prefix).unwrap_or("");
    let sub_path = sub_path.trim_start_matches('/');

    // Upload base = configured upload_dir (e.g. "static")
    let upload_base = root_dir.join(upload_dir);

    // If sub_path is not empty, append it (e.g. /static/lala)
    if sub_path.is_empty() {
        upload_base.clone()
    } else {
        upload_base.join(sub_path)
    }
}
