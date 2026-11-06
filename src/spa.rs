use axum::{
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "frontend/dist/"]
struct Frontend;

pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match Frontend::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let cache = if path.starts_with("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            (
                [
                    (header::CONTENT_TYPE, mime.as_ref().to_owned()),
                    (header::CACHE_CONTROL, cache.to_owned()),
                ],
                file.data,
            )
                .into_response()
        }
        // fall back to index.html
        None => match Frontend::get("index.html") {
            Some(index) => {
                ([(header::CONTENT_TYPE, "text/html".to_owned())], index.data).into_response()
            }
            None => StatusCode::NOT_FOUND.into_response(),
        },
    }
}
