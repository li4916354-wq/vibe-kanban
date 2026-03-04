use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{post, put},
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::DeploymentImpl;

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/open-remote-editor", post(open_remote_editor))
        .route(
            "/open-remote-editor/credentials",
            put(upsert_open_remote_editor_credentials),
        )
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct OpenRemoteEditorWithStoredCredentialsRequest {
    pub host_id: String,
    pub workspace_path: String,
    #[serde(default)]
    pub editor_type: Option<String>,
    pub relay_session_base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpsertOpenRemoteEditorCredentialsRequest {
    pub host_id: String,
    pub signing_session_id: String,
    pub private_key_jwk: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpsertOpenRemoteEditorCredentialsResponse {
    pub upserted: bool,
}

pub async fn open_remote_editor(
    State(deployment): State<DeploymentImpl>,
    Json(req): Json<OpenRemoteEditorWithStoredCredentialsRequest>,
) -> Response {
    handle_open_remote_editor(&deployment, req).await
}

pub async fn upsert_open_remote_editor_credentials(
    State(deployment): State<DeploymentImpl>,
    Json(req): Json<UpsertOpenRemoteEditorCredentialsRequest>,
) -> Response {
    match deployment
        .upsert_relay_host_credentials(req.host_id, req.signing_session_id, req.private_key_jwk)
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(UpsertOpenRemoteEditorCredentialsResponse { upserted: true }),
        )
            .into_response(),
        Err(error) => {
            tracing::error!(?error, "Failed to persist relay host credentials");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to persist relay host credentials"
                })),
            )
                .into_response()
        }
    }
}

pub async fn handle_open_remote_editor(
    deployment: &DeploymentImpl,
    req: OpenRemoteEditorWithStoredCredentialsRequest,
) -> Response {
    let Some(credentials) = deployment.get_relay_host_credentials(&req.host_id).await else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Open-in-IDE credentials are unavailable for host '{}'", req.host_id)
            })),
        )
            .into_response();
    };

    let service_req = desktop_bridge::service::OpenRemoteEditorRequest {
        workspace_path: req.workspace_path,
        editor_type: req.editor_type,
        relay_session_base_url: req.relay_session_base_url,
        signing_session_id: credentials.signing_session_id,
        private_key_jwk: credentials.private_key_jwk,
    };

    match deployment
        .desktop_bridge()
        .open_remote_editor(service_req)
        .await
    {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => {
            let status = if error.is_invalid_request() {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            tracing::error!(?error, "Open remote editor failed");
            (
                status,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response()
        }
    }
}
