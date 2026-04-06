use std::sync::Arc;

use kf2_proto::kf2::session_service_server::SessionService;
use kf2_proto::kf2::{
    CreateSessionRequest, CreateSessionResponse, DeleteSessionRequest, DeleteSessionResponse,
    GetSessionRequest, GetSessionResponse, ListSessionsRequest, ListSessionsResponse,
};
use tonic::{Request, Response, Status};

use crate::session;
use crate::AppState;

pub struct SessionServiceImpl {
    pub state: Arc<AppState>,
}

fn to_proto(s: session::Session) -> kf2_proto::kf2::Session {
    kf2_proto::kf2::Session {
        id: s.id,
        created_at: s.created_at,
        updated_at: s.updated_at,
    }
}

#[tonic::async_trait]
impl SessionService for SessionServiceImpl {
    async fn create_session(
        &self,
        _req: Request<CreateSessionRequest>,
    ) -> Result<Response<CreateSessionResponse>, Status> {
        let s = session::create_session(&self.state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(CreateSessionResponse {
            session: Some(to_proto(s)),
        }))
    }

    async fn get_session(
        &self,
        req: Request<GetSessionRequest>,
    ) -> Result<Response<GetSessionResponse>, Status> {
        let id = req.into_inner().id;
        let s = session::get_session(&self.state.db, id.clone())
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        match s {
            Some(s) => Ok(Response::new(GetSessionResponse {
                session: Some(to_proto(s)),
            })),
            None => Err(Status::not_found(format!("session '{id}' not found"))),
        }
    }

    async fn list_sessions(
        &self,
        _req: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let sessions = session::list_sessions(&self.state.db)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListSessionsResponse {
            sessions: sessions.into_iter().map(to_proto).collect(),
        }))
    }

    async fn delete_session(
        &self,
        req: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status> {
        let id = req.into_inner().id;
        let deleted = session::delete_session(&self.state.db, id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(DeleteSessionResponse { deleted }))
    }
}
