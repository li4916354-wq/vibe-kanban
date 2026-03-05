use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::Json as ResponseJson,
    routing::get,
};
use db::models::chat_session::{ChatSession, ChatSessionError, CreateChatSession, UpdateChatSession};
use deployment::Deployment;
use serde::Deserialize;
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Deserialize)]
pub struct ChatSessionQuery {
    pub project_id: Uuid,
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateChatSessionRequest {
    pub project_id: Uuid,
    pub title: Option<String>,
    pub executor: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
pub struct UpdateChatSessionRequest {
    pub title: Option<String>,
    pub pinned: Option<bool>,
}

impl From<ChatSessionError> for ApiError {
    fn from(err: ChatSessionError) -> Self {
        match err {
            ChatSessionError::Database(e) => ApiError::Database(e),
            ChatSessionError::NotFound => ApiError::NotFound("Chat session not found".to_string()),
            ChatSessionError::ProjectNotFound => ApiError::NotFound("Project not found".to_string()),
        }
    }
}

pub async fn list_chat_sessions(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<ChatSessionQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<db::models::chat_session::ChatSessionWithStatus>>>, ApiError> {
    let pool = &deployment.db().pool;
    let sessions = ChatSession::find_by_project_id_with_status(pool, query.project_id).await?;
    Ok(ResponseJson(ApiResponse::success(sessions)))
}

pub async fn get_chat_session(
    State(deployment): State<DeploymentImpl>,
    Path(session_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<ChatSession>>, ApiError> {
    let pool = &deployment.db().pool;
    let session = ChatSession::find_by_id(pool, session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Chat session not found".to_string()))?;
    Ok(ResponseJson(ApiResponse::success(session)))
}

pub async fn create_chat_session(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateChatSessionRequest>,
) -> Result<ResponseJson<ApiResponse<ChatSession>>, ApiError> {
    let pool = &deployment.db().pool;

    let session = ChatSession::create(
        pool,
        &CreateChatSession {
            project_id: payload.project_id,
            title: payload.title,
            executor: payload.executor,
        },
    )
    .await?;

    Ok(ResponseJson(ApiResponse::success(session)))
}

pub async fn update_chat_session(
    State(deployment): State<DeploymentImpl>,
    Path(session_id): Path<Uuid>,
    Json(payload): Json<UpdateChatSessionRequest>,
) -> Result<ResponseJson<ApiResponse<ChatSession>>, ApiError> {
    let pool = &deployment.db().pool;

    let session = ChatSession::update(
        pool,
        session_id,
        &UpdateChatSession {
            title: payload.title,
            pinned: payload.pinned,
        },
    )
    .await?;

    Ok(ResponseJson(ApiResponse::success(session)))
}

pub async fn delete_chat_session(
    State(deployment): State<DeploymentImpl>,
    Path(session_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let pool = &deployment.db().pool;
    ChatSession::delete(pool, session_id).await?;
    Ok(ResponseJson(ApiResponse::success(())))
}

pub fn router(_deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    Router::new()
        .route("/chat-sessions", get(list_chat_sessions).post(create_chat_session))
        .route(
            "/chat-sessions/{session_id}",
            get(get_chat_session)
                .put(update_chat_session)
                .delete(delete_chat_session),
        )
}
