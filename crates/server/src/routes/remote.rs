//! Remote Access Routes
//! HTTP API endpoints for remote access, tunnel, and TOTP functionality

use axum::{
    Json, Router,
    extract::State,
    response::{sse::Event, Sse},
    routing::{get, post},
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use services::services::remote_access::{
    RemoteAccessStatus, TunnelStatus, TotpStatus,
};
use std::{convert::Infallible, time::Duration};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use utils::response::ApiResponse;

use crate::DeploymentImpl;

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/remote/status", get(get_status))
        .route("/remote/enable", post(enable_remote))
        .route("/remote/disable", post(disable_remote))
        .route("/remote/tunnel/enable", post(enable_tunnel))
        .route("/remote/tunnel/disable", post(disable_tunnel))
        .route("/remote/qrcode", get(get_qrcode))
        .route("/remote/password", post(set_password))
        .route("/remote/totp/status", get(get_totp_status))
        .route("/remote/totp/generate", post(generate_totp_secret))
        .route("/remote/totp/bind", post(bind_totp))
        .route("/remote/totp/unbind", post(unbind_totp))
        .route("/remote/events", get(status_events))
}

#[derive(Debug, Deserialize)]
pub struct EnableRemoteRequest {
    pub port: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub struct SetPasswordRequest {
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct BindTotpRequest {
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct TotpSecretResponse {
    pub secret: String,
    pub qr_code: String,
}

#[derive(Debug, Serialize)]
pub struct QrCodeResponse {
    pub qr_code: String,
}

/// Get current remote access status
async fn get_status(
    State(deployment): State<DeploymentImpl>,
) -> Json<ApiResponse<RemoteAccessStatus>> {
    let status = deployment.remote_access().get_status().await;
    Json(ApiResponse::success(status))
}

/// Enable remote access
async fn enable_remote(
    State(deployment): State<DeploymentImpl>,
    Json(req): Json<EnableRemoteRequest>,
) -> Json<ApiResponse<RemoteAccessStatus>> {
    match deployment.remote_access().enable(req.port).await {
        Ok(status) => Json(ApiResponse::success(status)),
        Err(e) => Json(ApiResponse::error(&e.to_string())),
    }
}

/// Disable remote access
async fn disable_remote(
    State(deployment): State<DeploymentImpl>,
) -> Json<ApiResponse<()>> {
    match deployment.remote_access().disable().await {
        Ok(_) => Json(ApiResponse::success(())),
        Err(e) => Json(ApiResponse::error(&e.to_string())),
    }
}

/// Enable tunnel for internet access
async fn enable_tunnel(
    State(deployment): State<DeploymentImpl>,
) -> Json<ApiResponse<TunnelStatus>> {
    match deployment.remote_access().enable_tunnel().await {
        Ok(status) => Json(ApiResponse::success(status)),
        Err(e) => Json(ApiResponse::error(&e.to_string())),
    }
}

/// Disable tunnel
async fn disable_tunnel(
    State(deployment): State<DeploymentImpl>,
) -> Json<ApiResponse<()>> {
    match deployment.remote_access().disable_tunnel().await {
        Ok(_) => Json(ApiResponse::success(())),
        Err(e) => Json(ApiResponse::error(&e.to_string())),
    }
}

/// Get QR code for remote access
async fn get_qrcode(
    State(deployment): State<DeploymentImpl>,
) -> Json<ApiResponse<QrCodeResponse>> {
    match deployment.remote_access().generate_qr_code(false).await {
        Ok(qr_code) => Json(ApiResponse::success(QrCodeResponse { qr_code })),
        Err(e) => Json(ApiResponse::error(&e.to_string())),
    }
}

/// Set custom password for remote access
async fn set_password(
    State(deployment): State<DeploymentImpl>,
    Json(req): Json<SetPasswordRequest>,
) -> Json<ApiResponse<()>> {
    match deployment.remote_access().set_password(&req.password).await {
        Ok(_) => Json(ApiResponse::success(())),
        Err(e) => Json(ApiResponse::error(&e.to_string())),
    }
}

/// Get TOTP binding status
async fn get_totp_status(
    State(deployment): State<DeploymentImpl>,
) -> Json<ApiResponse<TotpStatus>> {
    let status = deployment.remote_access().get_totp_status().await;
    Json(ApiResponse::success(status))
}

/// Generate TOTP secret for binding
async fn generate_totp_secret(
    State(deployment): State<DeploymentImpl>,
) -> Json<ApiResponse<TotpSecretResponse>> {
    match deployment.remote_access().generate_totp_secret().await {
        Ok((secret, qr_code)) => Json(ApiResponse::success(TotpSecretResponse {
            secret,
            qr_code,
        })),
        Err(e) => Json(ApiResponse::error(&e.to_string())),
    }
}

/// Bind TOTP with verification code
async fn bind_totp(
    State(deployment): State<DeploymentImpl>,
    Json(req): Json<BindTotpRequest>,
) -> Json<ApiResponse<()>> {
    match deployment.remote_access().bind_totp(&req.code).await {
        Ok(_) => Json(ApiResponse::success(())),
        Err(e) => Json(ApiResponse::error(&e.to_string())),
    }
}

/// Unbind TOTP
async fn unbind_totp(
    State(deployment): State<DeploymentImpl>,
) -> Json<ApiResponse<()>> {
    match deployment.remote_access().unbind_totp().await {
        Ok(_) => Json(ApiResponse::success(())),
        Err(e) => Json(ApiResponse::error(&e.to_string())),
    }
}

/// SSE endpoint for real-time status updates
async fn status_events(
    State(deployment): State<DeploymentImpl>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = deployment.remote_access().subscribe_status();
    let stream = BroadcastStream::new(rx)
        .filter_map(|result| result.ok())
        .map(|status| {
            Ok(Event::default()
                .json_data(&status)
                .unwrap_or_else(|_| Event::default()))
        });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("keep-alive"),
    )
}
