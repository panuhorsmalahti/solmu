use axum::{
    Router,
    extract::Request,
    http::{HeaderValue, header},
    middleware::{self, Next},
    response::Response,
};
use std::path::Path;
use tower_http::services::{ServeDir, ServeFile};

pub fn router(api: Router, directory: &Path) -> Router {
    let index = ServeFile::new(directory.join("index.html"));
    let pages = Router::new()
        .route_service("/", index.clone())
        .route_service("/profile", index.clone())
        .route_service("/audit", index.clone())
        .route_service("/tasks", index.clone())
        .route_service("/webhooks", index.clone())
        .route_service("/threads/{thread_id}", index)
        .layer(middleware::from_fn(no_cache));
    api.merge(pages)
        .nest_service("/assets", ServeDir::new(directory.join("assets")))
}

async fn no_cache(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
