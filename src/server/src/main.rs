mod config;
mod db;
mod grpc;
mod session;

use std::sync::Arc;

use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as HttpBuilder;
use kf2_proto::kf2::session_service_server::SessionServiceServer;
use tower::Service;

pub struct AppState {
    pub db: db::DbPool,
    pub config: config::AppConfig,
}

#[tokio::main]
async fn main() {
    let config = config::load_config().expect("Failed to load configuration");

    let pool = db::create_pool(&config.database).expect("Failed to create database pool");
    db::run_migrations(&pool)
        .await
        .expect("Failed to run database migrations");

    let state = Arc::new(AppState {
        db: pool,
        config: config.clone(),
    });

    let grpc_svc = grpc::SessionServiceImpl {
        state: state.clone(),
    };
    let grpc_router = tonic::service::Routes::new(SessionServiceServer::new(grpc_svc))
        .into_axum_router();

    let app: axum::Router = axum::Router::new().merge(grpc_router);

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    eprintln!("Listening on {addr}");

    // Accept loop with h2c support for gRPC
    loop {
        let (stream, _) = listener.accept().await.expect("Failed to accept connection");
        let tower_svc = app.clone();
        tokio::spawn(async move {
            let hyper_svc = hyper::service::service_fn(move |req| {
                let mut svc = tower_svc.clone();
                async move { svc.call(req).await }
            });
            let builder = HttpBuilder::new(TokioExecutor::new());
            if let Err(e) = builder
                .serve_connection(TokioIo::new(stream), hyper_svc)
                .await
            {
                eprintln!("Connection error: {e}");
            }
        });
    }
}
