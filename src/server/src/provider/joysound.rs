use std::sync::Arc;

use crate::provider::ProviderSession;
use crate::provider::error::ProviderError;
use crate::provider::joysound_session::JoysoundProviderSession;
use crate::provider::types::*;

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
        let _cfg = config.ok_or_else(|| {
            ProviderError::AuthFailed("Joysound requires username/password".into())
        })?;
        // TODO: perform real upstream login using `_cfg.username` + `_cfg.password`.
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
