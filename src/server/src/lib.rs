pub mod api;
pub mod config;
pub mod db;
pub mod frontend;
pub mod provider;
pub mod provider_config;
pub mod repo;
pub mod session;

use std::sync::Arc;

use anyhow::{Context, Result};
use kf2_proto::kf2::provider_service_server::ProviderServiceServer;
use kf2_proto::kf2::session_manager_service_server::SessionManagerServiceServer;
use kf2_proto::kf2::session_service_server::SessionServiceServer;
use tokio::net::TcpListener;

use crate::config::AppConfig;
use crate::provider::cache::ProviderCache;
use crate::repo::diesel_impl::{DieselProviderConfigRepo, DieselSessionRepo};
use crate::repo::{ProviderConfigRepo, SessionRepo};

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

    let mut registry = provider::ProviderRegistry::new();
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
pub fn build_router(state: Arc<AppState>) -> axum::Router {
    let session_manager_svc = api::SessionManagerServiceImpl {
        state: state.clone(),
    };
    let session_svc = api::SessionServiceImpl {
        state: state.clone(),
    };
    let provider_svc = api::ProviderServiceImpl {
        state: state.clone(),
    };
    let grpc_router =
        tonic::service::Routes::new(SessionManagerServiceServer::new(session_manager_svc))
            .add_service(SessionServiceServer::new(session_svc))
            .add_service(ProviderServiceServer::new(provider_svc))
            .into_axum_router()
            .layer(tonic_web::GrpcWebLayer::new());

    let frontend_router = frontend::frontend_routes(&state.config);

    axum::Router::new()
        .merge(grpc_router)
        .merge(frontend_router)
}

/// Convenience entry point: build state, build the router, bind, and serve.
pub async fn run(config: AppConfig) -> Result<()> {
    let addr = config.server.listen_addr();
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

    use std::sync::Arc;

    use crate::config::{AppConfig, DatabaseConfig, FrontendConfig, ServerConfig};
    use crate::provider::cache::ProviderCache;
    use crate::provider::{Provider, ProviderRegistry};
    use crate::repo::diesel_impl::{DieselProviderConfigRepo, DieselSessionRepo};
    use crate::repo::{ProviderConfigRepo, SessionRepo};
    use crate::{AppState, db};

    fn dummy_config() -> AppConfig {
        AppConfig {
            database: DatabaseConfig {
                path: ":memory:".into(),
            },
            server: ServerConfig {
                host: "127.0.0.1".into(),
                port: 0,
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
        let pool = db::create_pool_in_memory(db_name).await.unwrap();
        let sessions: Arc<dyn SessionRepo> = Arc::new(DieselSessionRepo::new(pool.clone()));
        let provider_configs: Arc<dyn ProviderConfigRepo> =
            Arc::new(DieselProviderConfigRepo::new(pool));
        let mut registry = ProviderRegistry::new();
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
