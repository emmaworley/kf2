//! Integration test for the server startup sequence.
//!
//! Exercises `build_state` → `build_router` → `serve` end-to-end against an
//! in-memory database and an ephemeral TCP port, then makes a real HTTP
//! request to confirm the bound listener is actually serving traffic.

use anyhow::Context;
use server::db;
use server::{AppConfig, DatabaseConfig, FrontendConfig, ServerConfig};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tempfile::TempDir;
use tokio::net::TcpListener;

fn test_config(tmp: &TempDir, db_name: &str) -> AppConfig {
    // Point both SPA roots at the (empty) tempdir so ServeDir has a real
    // directory to mount; missing files will simply 404, which is fine.
    let root = tmp.path().to_string_lossy().into_owned();
    AppConfig {
        database: DatabaseConfig {
            path: db::test_support::in_memory_uri(db_name),
        },
        server: ServerConfig {
            listen_addr: "127.0.0.1:0".to_string(),
        },
        projector: FrontendConfig { root: root.clone() },
        remocon: FrontendConfig { root },
    }
}

#[tokio::test]
async fn server_starts_and_serves_requests() {
    let tmp = TempDir::new().expect("create tempdir");
    let config = test_config(&tmp, "startup_serves_requests");

    // Step 1: build_state runs migrations against the temp DB and registers
    // every provider — this is the bulk of the startup sequence.
    let state = server::build_app(config)
        .await
        .expect("build_state should succeed");

    // Step 2: build_router wires the gRPC services + frontend routes together.
    let app = server::build_router(state);

    // Step 3: bind on an ephemeral port so the test can run in parallel with
    // anything else and discover the actual address after the fact.
    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");

    // Step 4: serve in the background; abort once the assertions are done.
    let serve_handle =
        tokio::spawn(async move { axum::serve(listener, app).await.context("server error") });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("reqwest client");

    // Any well-formed HTTP response proves the accept loop is live and the
    // router is wired up. We don't care about the specific status — the SPA
    // dist dir is empty so a 404 is the expected (and acceptable) outcome.
    let response = client
        .get(format!("http://{addr}/projector/"))
        .send()
        .await
        .expect("server should answer the request");

    assert!(
        response.status().is_client_error() || response.status().is_success(),
        "unexpected status from startup probe: {}",
        response.status()
    );

    serve_handle.abort();
}

/// When a frontend's `root` is an `http://` URL, requests nested under that
/// frontend's prefix should be reverse-proxied to the upstream — including
/// request bodies and response bodies — so Vite HMR works end-to-end.
#[cfg(feature = "test-support")]
#[tokio::test]
async fn frontend_reverse_proxies_to_dev_server() {
    // Step 1: stand up a mock "Vite dev server" on an ephemeral port. Axum
    // strips the `/projector` prefix when nesting, so the upstream sees paths
    // without it — mirror that here.
    let mock = axum::Router::new()
        .route(
            "/index.html",
            axum::routing::get(|| async { "VITE-DEV-SERVER-OK" }),
        )
        .route(
            "/api/echo",
            axum::routing::post(|body: String| async move { format!("echo:{body}") }),
        );
    let mock_listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .expect("bind mock dev server");
    let mock_addr = mock_listener.local_addr().expect("mock local_addr");
    let mock_handle = tokio::spawn(async move {
        axum::serve(mock_listener, mock).await.expect("mock serve");
    });

    // Step 2: build a config whose projector points at the mock via http://,
    // which FrontendConfig::is_dev_server should detect and wire up the proxy.
    let tmp = TempDir::new().expect("create tempdir");
    let static_root = tmp.path().to_string_lossy().into_owned();
    let config = AppConfig {
        database: DatabaseConfig {
            path: db::test_support::in_memory_uri("startup_dev_proxy"),
        },
        server: ServerConfig {
            listen_addr: "127.0.0.1:0".to_string(),
        },
        projector: FrontendConfig {
            root: format!("http://{mock_addr}"),
        },
        // remocon stays in static-serve mode so we're exercising the mixed
        // configuration — one dev-proxied, one disk-served — in a single run.
        remocon: FrontendConfig { root: static_root },
    };

    let state = server::build_app(config)
        .await
        .expect("build_app should succeed");
    let app = server::build_router(state);

    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .expect("bind kf2 ephemeral port");
    let addr = listener.local_addr().expect("kf2 local_addr");
    let serve_handle =
        tokio::spawn(async move { axum::serve(listener, app).await.context("server error") });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("reqwest client");

    // Step 3: a plain GET under /projector should proxy through to the mock
    // and return its body verbatim.
    let resp = client
        .get(format!("http://{addr}/projector/index.html"))
        .send()
        .await
        .expect("GET /projector/index.html");
    assert_eq!(resp.status().as_u16(), 200);
    assert_eq!(
        resp.text().await.expect("read body"),
        "VITE-DEV-SERVER-OK",
        "projector GET should be served by the upstream dev server"
    );

    // Step 4: a POST with a body should forward the method AND the request
    // body — the dev_proxy implementation rewrites the method and reads the
    // body into the outgoing reqwest call, so this guards both code paths.
    let resp = client
        .post(format!("http://{addr}/projector/api/echo"))
        .body("ping")
        .send()
        .await
        .expect("POST /projector/api/echo");
    assert_eq!(resp.status().as_u16(), 200);
    assert_eq!(
        resp.text().await.expect("read body"),
        "echo:ping",
        "projector POST body should be forwarded to the upstream dev server"
    );

    serve_handle.abort();
    mock_handle.abort();
}
