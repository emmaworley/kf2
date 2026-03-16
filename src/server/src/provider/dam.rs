use std::sync::Arc;

use crate::provider::ProviderSession;
use crate::provider::dam_session::DamProviderSession;
use crate::provider::error::ProviderError;
use crate::provider::types::*;

pub struct DamProvider;

impl DamProvider {
    pub const METADATA: ProviderMetadata = ProviderMetadata {
        id: ProviderId::Dam,
        name: "DAM",
        capabilities: &[Capability::Search, Capability::Scoring],
        requires_configuration: true,
    };

    pub async fn configure(
        &self,
        config: Option<&ProviderConfig>,
    ) -> Result<Arc<dyn ProviderSession>, ProviderError> {
        let _cfg = config
            .ok_or_else(|| ProviderError::AuthFailed("DAM requires username/password".into()))?;
        // TODO: perform real upstream login using `_cfg.username` + `_cfg.password`.
        // For now, hand back a session with a placeholder token.
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .expect("failed to build HTTP client");
        Ok(Arc::new(DamProviderSession::new(
            client,
            "dummy-token".into(),
        )))
    }
}
