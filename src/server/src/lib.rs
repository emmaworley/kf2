pub mod api;
pub mod cli;
pub mod db;
pub mod models;
pub mod provider;
pub mod repo;

use std::sync::Arc;

use crate::provider::cache::ProviderCache;
use crate::repo::diesel_impl::{DieselProviderConfigRepo, DieselSessionRepo};
use crate::repo::{ProviderConfigRepo, SessionRepo};
use anyhow::{Context, Result};
use axum::Router;
use axum::body::Body;
use axum::http::{Response, StatusCode};
use axum::response::IntoResponse;
use kf2_proto::kf2::provider_service_server::ProviderServiceServer;
use kf2_proto::kf2::session_manager_service_server::SessionManagerServiceServer;
use kf2_proto::kf2::session_service_server::SessionServiceServer;
use serde::Deserialize;
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};
//
// App-level config structs
//

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub projector: FrontendConfig,
    pub remocon: FrontendConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub listen_addr: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FrontendConfig {
    pub root: String,
}

impl FrontendConfig {
    pub fn is_http_endpoint(&self) -> bool {
        let r = self.root.trim_start();
        r.starts_with("http://") || r.starts_with("https://")
    }
}

//
// App-level state orchestration
//

pub struct AppState {
    pub sessions: Arc<dyn SessionRepo>,
    pub provider_configs: Arc<dyn ProviderConfigRepo>,
    pub config: AppConfig,
    pub providers: provider::ProviderRegistry,
    pub provider_cache: ProviderCache,
}

/// Initialize the database, run migrations, and register all providers.
pub async fn build_app(config: AppConfig) -> Result<Arc<AppState>> {
    let pool = db::create_pool(&config.database)
        .map_err(|e| anyhow::anyhow!("Failed to create database pool: {e}"))?;
    db::run_migrations(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run database migrations: {e}"))?;

    let sessions: Arc<dyn SessionRepo> = Arc::new(DieselSessionRepo::new(pool.clone()));
    let provider_configs: Arc<dyn ProviderConfigRepo> =
        Arc::new(DieselProviderConfigRepo::new(pool));

    let mut registry = provider::ProviderRegistry::default();
    registry.register(Arc::new(provider::Provider::Dam(
        provider::dam::DamProvider,
    )));
    registry.register(Arc::new(provider::Provider::Joysound(
        provider::joysound::JoysoundProvider,
    )));
    if let Some(yt) = provider::youtube::YouTubeProvider::new() {
        eprintln!(
            "YouTube provider enabled (yt-dlp {} at {})",
            yt.ytdlp_version,
            yt.ytdlp_path.display()
        );
        registry.register(Arc::new(provider::Provider::YouTube(yt)));
    } else {
        eprintln!("yt-dlp not found, YouTube provider disabled");
    }

    Ok(Arc::new(AppState {
        sessions,
        provider_configs,
        config,
        providers: registry,
        provider_cache: ProviderCache::new(),
    }))
}

/// Build the top-level axum router (gRPC-web services + frontend SPAs).
pub fn build_router(state: Arc<AppState>) -> Router {
    let grpc_router = grpc_routes(state.clone());
    let frontend_router = frontend_routes(&state.config);

    Router::new().merge(grpc_router).merge(frontend_router)
}

fn grpc_routes(state: Arc<AppState>) -> Router {
    let session_manager_svc = api::SessionManagerServiceImpl {
        state: state.clone(),
    };
    let session_svc = api::SessionServiceImpl {
        state: state.clone(),
    };
    let provider_svc = api::ProviderServiceImpl {
        state: state.clone(),
    };
    tonic::service::Routes::new(SessionManagerServiceServer::new(session_manager_svc))
        .add_service(SessionServiceServer::new(session_svc))
        .add_service(ProviderServiceServer::new(provider_svc))
        .into_axum_router()
        .layer(tonic_web::GrpcWebLayer::new())
}

pub fn frontend_routes(config: &AppConfig) -> Router {
    Router::new()
        .nest("/projector", spa_router(&config.projector))
        .nest("/remocon", spa_router(&config.remocon))
}

fn spa_router(frontend: &FrontendConfig) -> Router {
    if frontend.is_http_endpoint() {
        dev_proxy_router(&frontend.root)
    } else {
        serve_dir_router(&frontend.root)
    }
}

fn serve_dir_router(dist_path: &str) -> Router {
    let index = format!("{}/index.html", dist_path);
    Router::new().fallback_service(ServeDir::new(dist_path).fallback(ServeFile::new(index)))
}

fn dev_proxy_router(upstream_url: &str) -> Router {
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
            let req_body = reqwest::Body::wrap_stream(body.into_data_stream());

            match client
                .request(parts.method, &url)
                .headers(parts.headers)
                .body(req_body)
                .send()
                .await
            {
                Ok(resp) => {
                    let mut builder = Response::builder().status(resp.status());
                    if let Some(headers) = builder.headers_mut() {
                        *headers = resp.headers().clone();
                    }
                    builder
                        .body(Body::from_stream(resp.bytes_stream()))
                        .expect("response builder with no header mutations cannot fail")
                        .into_response()
                }
                Err(_) => (StatusCode::BAD_GATEWAY, "Dev server unreachable").into_response(),
            }
        }
    })
}

/// Convenience entry point: build state, build the router, bind, and serve.
pub async fn run(config: AppConfig) -> Result<()> {
    let addr = config.server.listen_addr.clone();
    let state = build_app(config).await?;
    let app = build_router(state);
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;
    eprintln!("Listening on {addr}");
    axum::serve(listener, app).await.context("server error")
}

#[cfg(test)]
pub mod test_support {
    //! Helpers for constructing an `AppState` in unit tests without touching
    //! the real filesystem config or spinning up a full `build_app`.

    use crate::db::test_support::create_pool_in_memory;
    use crate::provider::cache::ProviderCache;
    use crate::provider::{Provider, ProviderRegistry};
    use crate::repo::diesel_impl::DieselProviderConfigRepo;
    use crate::repo::diesel_impl::DieselSessionRepo;
    use crate::repo::{ProviderConfigRepo, SessionRepo};
    use crate::{AppConfig, AppState, DatabaseConfig, FrontendConfig, ServerConfig};
    use std::sync::Arc;

    fn dummy_config() -> AppConfig {
        AppConfig {
            database: DatabaseConfig {
                path: ":memory:".into(),
            },
            server: ServerConfig {
                listen_addr: "127.0.0.1:0".into(),
            },
            projector: FrontendConfig {
                root: String::new(),
            },
            remocon: FrontendConfig {
                root: String::new(),
            },
        }
    }

    /// Build a fully-migrated `AppState` backed by a named in-memory SQLite
    /// database, with the given providers registered.
    pub async fn test_app_state(db_name: &str, providers: Vec<Arc<Provider>>) -> Arc<AppState> {
        let pool = create_pool_in_memory(db_name).await.unwrap();
        let sessions: Arc<dyn SessionRepo> = Arc::new(DieselSessionRepo::new(pool.clone()));
        let provider_configs: Arc<dyn ProviderConfigRepo> =
            Arc::new(DieselProviderConfigRepo::new(pool));
        let mut registry = ProviderRegistry::default();
        for p in providers {
            registry.register(p);
        }
        Arc::new(AppState {
            sessions,
            provider_configs,
            config: dummy_config(),
            providers: registry,
            provider_cache: ProviderCache::new(),
        })
    }
}
