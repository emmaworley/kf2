use tonic::Status;

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("not supported by this provider")]
    NotSupported,

    #[error("authentication failed: {0}")]
    AuthFailed(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("rate limited, retry after {retry_after_secs:?}s")]
    RateLimited { retry_after_secs: Option<u64> },

    #[error("upstream error: {0}")]
    Upstream(String),

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<ProviderError> for Status {
    fn from(e: ProviderError) -> Self {
        match &e {
            ProviderError::NotSupported => Status::unimplemented(e.to_string()),
            ProviderError::AuthFailed(_) => Status::unauthenticated(e.to_string()),
            ProviderError::NotFound(_) => Status::not_found(e.to_string()),
            ProviderError::RateLimited { .. } => Status::resource_exhausted(e.to_string()),
            _ => Status::internal(e.to_string()),
        }
    }
}
