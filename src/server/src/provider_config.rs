use crate::provider::types::{ProviderConfig, ProviderId};

/// A row in the `session_provider_config` table.
#[derive(Debug, Clone)]
pub struct ProviderConfigRow {
    pub session_id: String,
    pub provider_id: ProviderId,
    pub config: ProviderConfig,
}

impl From<&ProviderConfigRow> for ProviderConfig {
    fn from(row: &ProviderConfigRow) -> Self {
        row.config.clone()
    }
}
