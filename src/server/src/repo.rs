use async_trait::async_trait;

use crate::provider::types::{ProviderConfig, ProviderId};
use crate::provider_config::ProviderConfigRow;
use crate::session::Session;

pub mod diesel_impl;
#[allow(clippy::all, unused_qualifications)]
pub(crate) mod schema;

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("database error: {0}")]
    Database(#[from] diesel::result::Error),
    #[error("connection pool error: {0}")]
    Pool(#[from] diesel::r2d2::PoolError),
    #[error("blocking task canceled: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),
    #[error("config json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("provider id parse error: {0}")]
    ProviderId(String),
    #[error("exhausted unique-id retries")]
    IdRetriesExhausted,
}

impl From<RepoError> for tonic::Status {
    fn from(e: RepoError) -> Self {
        tonic::Status::internal(e.to_string())
    }
}

#[async_trait]
pub trait SessionRepo: Send + Sync {
    async fn create(&self) -> Result<Session, RepoError>;
    async fn get(&self, id: &str) -> Result<Option<Session>, RepoError>;
    async fn list(&self) -> Result<Vec<Session>, RepoError>;
    async fn delete(&self, id: &str) -> Result<bool, RepoError>;
}

#[async_trait]
pub trait ProviderConfigRepo: Send + Sync {
    async fn upsert(
        &self,
        session_id: &str,
        provider_id: ProviderId,
        config: ProviderConfig,
    ) -> Result<(), RepoError>;

    async fn get(
        &self,
        session_id: &str,
        provider_id: ProviderId,
    ) -> Result<Option<ProviderConfigRow>, RepoError>;

    async fn list_for_session(&self, session_id: &str)
    -> Result<Vec<ProviderConfigRow>, RepoError>;

    async fn delete(&self, session_id: &str, provider_id: ProviderId) -> Result<bool, RepoError>;
}
