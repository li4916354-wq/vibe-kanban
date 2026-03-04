use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{post, put},
};
use deployment::Deployment;
use tower_http::cors::CorsLayer;

use crate::{
    DeploymentImpl,
    routes::open_remote_editor::{
        OpenRemoteEditorWithStoredCredentialsRequest, UpsertOpenRemoteEditorCredentialsRequest,
    },
};

pub fn router(deployment: DeploymentImpl) -> Router {
    Router::new()
        .route("/api/open-remote-editor", post(open_remote_editor_bridge))
        .route(
            "/api/open-remote-editor/credentials",
            put(upsert_open_remote_editor_credentials_bridge),
        )
        .layer(CorsLayer::permissive())
        .with_state(deployment)
}

async fn open_remote_editor_bridge(
    State(deployment): State<DeploymentImpl>,
    axum::Json(req): axum::Json<OpenRemoteEditorWithStoredCredentialsRequest>,
) -> Response {
    forward_to_main_api(
        &deployment,
        reqwest::Method::POST,
        "/api/open-remote-editor",
        req,
    )
    .await
}

async fn upsert_open_remote_editor_credentials_bridge(
    State(deployment): State<DeploymentImpl>,
    axum::Json(req): axum::Json<UpsertOpenRemoteEditorCredentialsRequest>,
) -> Response {
    forward_to_main_api(
        &deployment,
        reqwest::Method::PUT,
        "/api/open-remote-editor/credentials",
        req,
    )
    .await
}

async fn forward_to_main_api<T: serde::Serialize>(
    deployment: &DeploymentImpl,
    method: reqwest::Method,
    path: &str,
    body: T,
) -> Response {
    let Some(main_port) = deployment.server_info().get_port().await else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Main API port unavailable" })),
        )
            .into_response();
    };

    let url = format!("http://127.0.0.1:{main_port}{path}");
    let client = reqwest::Client::new();

    match client.request(method, url).json(&body).send().await {
        Ok(response) => {
            let status = response.status();
            match response.json::<serde_json::Value>().await {
                Ok(body) => (status, Json(body)).into_response(),
                Err(error) => (
                    StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({
                        "error": format!("Bridge response parse failed: {error}")
                    })),
                )
                    .into_response(),
            }
        }
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": format!("Bridge forward failed: {error}")
            })),
        )
            .into_response(),
    }
}
