use std::sync::Arc;

use kf2_proto::kf2::provider_service_server::ProviderService;
use kf2_proto::kf2::{self as pb};
use tonic::{Request, Response, Status};

use crate::AppState;
use crate::provider::types::{Capability, ProviderId};

pub struct ProviderServiceImpl {
    pub state: Arc<AppState>,
}

// ---------------------------------------------------------------------------
// Proto <-> domain conversions
// ---------------------------------------------------------------------------

impl TryFrom<i32> for ProviderId {
    type Error = Status;

    fn try_from(pt: i32) -> Result<Self, Status> {
        match pb::ProviderType::try_from(pt) {
            Ok(pb::ProviderType::ProviderDam) => Ok(ProviderId::Dam),
            Ok(pb::ProviderType::ProviderJoysound) => Ok(ProviderId::Joysound),
            Ok(pb::ProviderType::ProviderYoutube) => Ok(ProviderId::YouTube),
            _ => Err(Status::invalid_argument("unknown provider type")),
        }
    }
}

impl From<ProviderId> for i32 {
    fn from(id: ProviderId) -> Self {
        let pt = match id {
            ProviderId::Dam => pb::ProviderType::ProviderDam,
            ProviderId::Joysound => pb::ProviderType::ProviderJoysound,
            ProviderId::YouTube => pb::ProviderType::ProviderYoutube,
        };
        pt as i32
    }
}

impl From<Capability> for i32 {
    fn from(c: Capability) -> Self {
        let ct = match c {
            Capability::Search => pb::CapabilityType::CapabilitySearch,
            Capability::Lyrics => pb::CapabilityType::CapabilityLyrics,
            Capability::Scoring => pb::CapabilityType::CapabilityScoring,
        };
        ct as i32
    }
}

// ---------------------------------------------------------------------------
// gRPC implementation
// ---------------------------------------------------------------------------

#[tonic::async_trait]
impl ProviderService for ProviderServiceImpl {
    async fn list_providers(
        &self,
        _req: Request<pb::ListProvidersRequest>,
    ) -> Result<Response<pb::ListProvidersResponse>, Status> {
        let providers = self
            .state
            .providers
            .all()
            .map(|p| {
                let m = p.metadata();
                pb::ProviderInfo {
                    provider: m.id.into(),
                    name: m.name.to_string(),
                    capabilities: m.capabilities.iter().copied().map(Into::into).collect(),
                }
            })
            .collect();
        Ok(Response::new(pb::ListProvidersResponse { providers }))
    }
}
