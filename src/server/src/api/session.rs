use std::sync::Arc;

use kf2_proto::kf2::session_service_server::SessionService;
use kf2_proto::kf2::{
    ConfigureProviderRequest, ConfigureProviderResponse, GetProviderStatusRequest,
    GetProviderStatusResponse, ListConfiguredProvidersRequest, ListConfiguredProvidersResponse,
    ProviderStatus, UnconfigureProviderRequest, UnconfigureProviderResponse,
};
use tonic::{Request, Response, Status};

use crate::AppState;
use crate::provider::types::{ProviderConfig, ProviderId};

pub struct SessionServiceImpl {
    pub state: Arc<AppState>,
}

fn config_username(config: &ProviderConfig) -> Option<String> {
    match config {
        ProviderConfig::BasicAuth { username, .. } => Some(username.clone()),
    }
}

impl SessionServiceImpl {
    async fn ensure_session_exists(&self, session_id: &str) -> Result<(), Status> {
        let existing = self.state.sessions.get(session_id).await?;
        if existing.is_none() {
            return Err(Status::not_found(format!(
                "session '{session_id}' not found"
            )));
        }
        Ok(())
    }
}

#[tonic::async_trait]
impl SessionService for SessionServiceImpl {
    async fn configure_provider(
        &self,
        req: Request<ConfigureProviderRequest>,
    ) -> Result<Response<ConfigureProviderResponse>, Status> {
        let inner = req.into_inner();
        let provider_id = ProviderId::try_from(inner.provider)?;
        self.ensure_session_exists(&inner.session_id).await?;

        // Resolve the provider factory from the registry.
        let factory = self
            .state
            .providers
            .get(provider_id)
            .ok_or_else(|| Status::not_found("provider not available on this server"))?
            .clone();

        let config = ProviderConfig::BasicAuth {
            username: inner.username,
            password: inner.password,
        };

        // Upsert first so a concurrent data RPC on the same (session, provider)
        // has a row to read. We'll roll it back if eager-validate fails.
        self.state
            .provider_configs
            .upsert(&inner.session_id, provider_id, config.clone())
            .await?;

        // Eager-validate by calling into the factory.
        match factory.configure(Some(&config)).await {
            Ok(configured) => {
                self.state
                    .provider_cache
                    .insert(&inner.session_id, provider_id, configured)
                    .await;
                Ok(Response::new(ConfigureProviderResponse {}))
            }
            Err(e) => {
                // Roll back the DB row so we don't leave orphaned credentials
                // that we already know don't work.
                let _ = self
                    .state
                    .provider_configs
                    .delete(&inner.session_id, provider_id)
                    .await;
                Err(Status::from(e))
            }
        }
    }

    async fn unconfigure_provider(
        &self,
        req: Request<UnconfigureProviderRequest>,
    ) -> Result<Response<UnconfigureProviderResponse>, Status> {
        let inner = req.into_inner();
        let provider_id = ProviderId::try_from(inner.provider)?;
        self.ensure_session_exists(&inner.session_id).await?;

        let removed = self
            .state
            .provider_configs
            .delete(&inner.session_id, provider_id)
            .await?;

        self.state
            .provider_cache
            .evict(&inner.session_id, provider_id)
            .await;

        Ok(Response::new(UnconfigureProviderResponse { removed }))
    }

    async fn get_provider_status(
        &self,
        req: Request<GetProviderStatusRequest>,
    ) -> Result<Response<GetProviderStatusResponse>, Status> {
        let inner = req.into_inner();
        let provider_id = ProviderId::try_from(inner.provider)?;
        self.ensure_session_exists(&inner.session_id).await?;

        let config = self
            .state
            .provider_configs
            .get(&inner.session_id, provider_id)
            .await?;

        let status = ProviderStatus {
            provider: inner.provider,
            is_configured: config.is_some(),
            username: config.and_then(|c| config_username(&c)),
        };
        Ok(Response::new(GetProviderStatusResponse {
            status: Some(status),
        }))
    }

    async fn list_configured_providers(
        &self,
        req: Request<ListConfiguredProvidersRequest>,
    ) -> Result<Response<ListConfiguredProvidersResponse>, Status> {
        let inner = req.into_inner();
        self.ensure_session_exists(&inner.session_id).await?;

        let rows = self
            .state
            .provider_configs
            .list_for_session(&inner.session_id)
            .await?;

        let providers = rows
            .into_iter()
            .map(|(provider_id, config)| ProviderStatus {
                provider: provider_id.into(),
                is_configured: true,
                username: config_username(&config),
            })
            .collect();
        Ok(Response::new(ListConfiguredProvidersResponse { providers }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    use kf2_proto::kf2::ProviderType;

    use crate::provider::Provider;
    use crate::provider::mock::{MockProvider, MockProviderControl};
    use crate::test_support::test_app_state;

    async fn setup(db_name: &str) -> (SessionServiceImpl, String, Arc<MockProviderControl>) {
        let (mock_provider, control) = MockProvider::new(ProviderId::Dam);
        let state = test_app_state(db_name, vec![Arc::new(Provider::Mock(mock_provider))]).await;
        let svc = SessionServiceImpl {
            state: state.clone(),
        };
        let session = state.sessions.create().await.unwrap();
        (svc, session.id, control)
    }

    #[tokio::test]
    async fn configure_provider_happy_path_persists_and_caches() {
        let (svc, session_id, control) = setup("svc_configure_happy").await;

        let resp = svc
            .configure_provider(Request::new(ConfigureProviderRequest {
                session_id: session_id.clone(),
                provider: ProviderType::ProviderDam as i32,
                username: "alice".into(),
                password: "hunter2".into(),
            }))
            .await
            .unwrap();
        let _ = resp.into_inner();

        // DB row was written.
        let config = svc
            .state
            .provider_configs
            .get(&session_id, ProviderId::Dam)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(config_username(&config).as_deref(), Some("alice"));

        // Cache has an entry.
        assert!(
            svc.state
                .provider_cache
                .get(&session_id, ProviderId::Dam)
                .await
                .is_some()
        );

        // Mock saw the eager-validate call.
        assert_eq!(control.configure_success_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn configure_provider_eager_validate_failure_leaves_no_row() {
        let (svc, session_id, control) = setup("svc_configure_fail").await;
        control.configure_always_fail.store(true, Ordering::SeqCst);

        let err = svc
            .configure_provider(Request::new(ConfigureProviderRequest {
                session_id: session_id.clone(),
                provider: ProviderType::ProviderDam as i32,
                username: "alice".into(),
                password: "hunter2".into(),
            }))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);

        // DB row was rolled back.
        let row = svc
            .state
            .provider_configs
            .get(&session_id, ProviderId::Dam)
            .await
            .unwrap();
        assert!(row.is_none());

        // Cache has no entry.
        assert!(
            svc.state
                .provider_cache
                .get(&session_id, ProviderId::Dam)
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn unconfigure_provider_evicts_cache_and_row() {
        let (svc, session_id, _control) = setup("svc_unconfigure").await;

        svc.configure_provider(Request::new(ConfigureProviderRequest {
            session_id: session_id.clone(),
            provider: ProviderType::ProviderDam as i32,
            username: "alice".into(),
            password: "hunter2".into(),
        }))
        .await
        .unwrap();

        let resp = svc
            .unconfigure_provider(Request::new(UnconfigureProviderRequest {
                session_id: session_id.clone(),
                provider: ProviderType::ProviderDam as i32,
            }))
            .await
            .unwrap();
        assert!(resp.into_inner().removed);

        let row = svc
            .state
            .provider_configs
            .get(&session_id, ProviderId::Dam)
            .await
            .unwrap();
        assert!(row.is_none());
        assert!(
            svc.state
                .provider_cache
                .get(&session_id, ProviderId::Dam)
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn get_provider_status_reflects_db_state() {
        let (svc, session_id, _control) = setup("svc_status").await;

        // Unconfigured first.
        let status = svc
            .get_provider_status(Request::new(GetProviderStatusRequest {
                session_id: session_id.clone(),
                provider: ProviderType::ProviderDam as i32,
            }))
            .await
            .unwrap()
            .into_inner()
            .status
            .unwrap();
        assert!(!status.is_configured);
        assert!(status.username.is_none());

        // After configure.
        svc.configure_provider(Request::new(ConfigureProviderRequest {
            session_id: session_id.clone(),
            provider: ProviderType::ProviderDam as i32,
            username: "alice".into(),
            password: "hunter2".into(),
        }))
        .await
        .unwrap();

        let status = svc
            .get_provider_status(Request::new(GetProviderStatusRequest {
                session_id: session_id.clone(),
                provider: ProviderType::ProviderDam as i32,
            }))
            .await
            .unwrap()
            .into_inner()
            .status
            .unwrap();
        assert!(status.is_configured);
        assert_eq!(status.username.as_deref(), Some("alice"));
    }

    #[tokio::test]
    async fn list_configured_providers_scoped_to_session() {
        let (svc, session_a, _control) = setup("svc_list").await;
        // A second session in the same DB.
        let session_b = svc.state.sessions.create().await.unwrap().id;

        svc.configure_provider(Request::new(ConfigureProviderRequest {
            session_id: session_a.clone(),
            provider: ProviderType::ProviderDam as i32,
            username: "alice".into(),
            password: "hunter2".into(),
        }))
        .await
        .unwrap();

        let a_list = svc
            .list_configured_providers(Request::new(ListConfiguredProvidersRequest {
                session_id: session_a.clone(),
            }))
            .await
            .unwrap()
            .into_inner()
            .providers;
        assert_eq!(a_list.len(), 1);
        assert_eq!(a_list[0].username.as_deref(), Some("alice"));

        let b_list = svc
            .list_configured_providers(Request::new(ListConfiguredProvidersRequest {
                session_id: session_b,
            }))
            .await
            .unwrap()
            .into_inner()
            .providers;
        assert!(b_list.is_empty());
    }
}
