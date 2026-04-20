use std::sync::Arc;

use crate::provider::ProviderSession;
use crate::provider::error::ProviderError;
use crate::provider::joysound_session::JoysoundProviderSession;
use crate::provider::types::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JoysoundConfig {
    pub username: String,
    pub password: String,
}

impl TryFrom<&ProviderConfig> for JoysoundConfig {
    type Error = serde_json::Error;

    fn try_from(value: &ProviderConfig) -> Result<Self, Self::Error> {
        serde_json::from_value(value.0.clone())
    }
}

impl From<JoysoundConfig> for ProviderConfig {
    fn from(cfg: JoysoundConfig) -> Self {
        ProviderConfig(serde_json::to_value(cfg).expect("JoysoundConfig is always serializable"))
    }
}

pub struct JoysoundProvider;

impl JoysoundProvider {
    pub const METADATA: ProviderMetadata = ProviderMetadata {
        id: ProviderId::Joysound,
        name: "Joysound",
        capabilities: &[Capability::Search, Capability::Lyrics],
        requires_configuration: true,
    };

    pub async fn configure(
        &self,
        config: Option<&ProviderConfig>,
    ) -> Result<Arc<dyn ProviderSession>, ProviderError> {
        let config = config.ok_or_else(|| {
            ProviderError::AuthFailed("Joysound requires username/password".into())
        })?;
        let _cfg = JoysoundConfig::try_from(config).map_err(|e| {
            ProviderError::AuthFailed(format!("Joysound expects username/password config: {e}"))
        })?;
        // TODO: perform real upstream login using the config's username/password.
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .expect("failed to build HTTP client");
        Ok(Arc::new(JoysoundProviderSession::new(
            client,
            "dummy-token".into(),
        )))
    }
}
