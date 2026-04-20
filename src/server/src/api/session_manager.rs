use std::sync::Arc;

use kf2_proto::kf2::session_manager_service_server::SessionManagerService;
use kf2_proto::kf2::{
    CreateSessionRequest, CreateSessionResponse, DeleteSessionRequest, DeleteSessionResponse,
    GetSessionRequest, GetSessionResponse, ListSessionsRequest, ListSessionsResponse,
};
use tonic::{Request, Response, Status};

use crate::{AppState, models};
pub struct SessionManagerServiceImpl {
    pub state: Arc<AppState>,
}

impl From<models::Session> for kf2_proto::kf2::Session {
    fn from(s: models::Session) -> Self {
        Self {
            id: s.id,
            created_at: s.created_at.and_utc().to_rfc3339(),
            updated_at: s.updated_at.and_utc().to_rfc3339(),
        }
    }
}

#[tonic::async_trait]
impl SessionManagerService for SessionManagerServiceImpl {
    async fn create_session(
        &self,
        _req: Request<CreateSessionRequest>,
    ) -> Result<Response<CreateSessionResponse>, Status> {
        let s = self.state.sessions.create().await?;
        Ok(Response::new(CreateSessionResponse {
            session: Some(s.into()),
        }))
    }

    async fn get_session(
        &self,
        req: Request<GetSessionRequest>,
    ) -> Result<Response<GetSessionResponse>, Status> {
        let id = req.into_inner().id;
        let s = self.state.sessions.get(&id).await?;
        match s {
            Some(s) => Ok(Response::new(GetSessionResponse {
                session: Some(s.into()),
            })),
            None => Err(Status::not_found(format!("session '{id}' not found"))),
        }
    }

    async fn list_sessions(
        &self,
        _req: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let sessions = self.state.sessions.list().await?;
        Ok(Response::new(ListSessionsResponse {
            sessions: sessions.into_iter().map(Into::into).collect(),
        }))
    }

    async fn delete_session(
        &self,
        req: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status> {
        let id = req.into_inner().id;
        let deleted = self.state.sessions.delete(&id).await?;
        Ok(Response::new(DeleteSessionResponse { deleted }))
    }
}
