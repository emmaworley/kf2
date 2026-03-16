use axum::Router;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use tower_http::services::{ServeDir, ServeFile};

use crate::config::{AppConfig, FrontendConfig};

/// Build SPA-serving routes for both frontends.
///
/// When a frontend's `root` is a filesystem path, serves static files from the
/// dist directory with fallback to index.html for client-side routing. When
/// `root` is an `http://` / `https://` URL, reverse-proxies requests to the
/// Vite dev server for HMR support.
pub fn frontend_routes(config: &AppConfig) -> Router {
    Router::new()
        .nest("/projector", spa_routes(&config.projector))
        .nest("/remocon", spa_routes(&config.remocon))
}

fn spa_routes(frontend: &FrontendConfig) -> Router {
    if frontend.is_dev_server() {
        dev_proxy_routes(&frontend.root)
    } else {
        static_spa_routes(&frontend.root)
    }
}

/// Serve a built SPA from disk with index.html fallback for client-side routing.
fn static_spa_routes(dist_path: &str) -> Router {
    let index = format!("{}/index.html", dist_path);
    Router::new().fallback_service(ServeDir::new(dist_path).fallback(ServeFile::new(index)))
}

/// Reverse-proxy all requests to a Vite dev server.
///
/// reqwest 0.12 re-exports `http::Method`, `http::StatusCode`, and
/// `http::HeaderMap`, so request method + headers forward unchanged and the
/// response can be assembled via axum's tuple `IntoResponse` impl instead of
/// hand-building a `Response`.
fn dev_proxy_routes(upstream_url: &str) -> Router {
    let client = reqwest::Client::new();
    let base_url = upstream_url.trim_end_matches('/').to_string();

    Router::new().fallback(move |req: axum::extract::Request| {
        let client = client.clone();
        let base_url = base_url.clone();
        async move {
            let (parts, body) = req.into_parts();
            // `Uri`'s `Display` prints `/path?query` when scheme/authority are
            // absent, which is always the case for a request axum has routed.
            let url = format!("{base_url}{}", parts.uri);
            let body = axum::body::to_bytes(body, usize::MAX)
                .await
                .unwrap_or_default();

            match client
                .request(parts.method, &url)
                .headers(parts.headers)
                .body(body)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    let headers = resp.headers().clone();
                    let bytes = resp.bytes().await.unwrap_or_default();
                    (status, headers, bytes).into_response()
                }
                Err(_) => (StatusCode::BAD_GATEWAY, "Dev server unreachable").into_response(),
            }
        }
    })
}
