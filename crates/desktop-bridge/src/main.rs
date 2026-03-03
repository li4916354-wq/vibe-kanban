mod signing;
mod ssh_config;
mod tunnel;

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use clap::Parser;
use serde::Deserialize;
use tower_http::cors::CorsLayer;

use crate::{signing::SigningContext, tunnel::TunnelManager};

#[derive(Parser)]
#[command(
    name = "desktop-bridge",
    about = "Local bridge for remote IDE opening via relay tunnel"
)]
struct Cli {
    /// Local HTTP API port
    #[arg(long, default_value = "15147", env = "BRIDGE_PORT")]
    port: u16,
}

struct AppState {
    tunnel_manager: TunnelManager,
}

#[derive(Deserialize)]
struct OpenRemoteEditorRequest {
    workspace_path: String,
    #[serde(default)]
    editor_type: Option<String>,
    /// Relay proxy session URL (e.g. https://relay.example.com/relay/h/{host_id}/s/{session_id})
    relay_session_base_url: String,
    /// Ed25519 signing session ID
    signing_session_id: String,
    /// Ed25519 private key in JWK format
    private_key_jwk: serde_json::Value,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "desktop_bridge=info".into()),
        )
        .init();

    let cli = Cli::parse();

    let state = Arc::new(AppState {
        tunnel_manager: TunnelManager::new(),
    });

    let app = Router::new()
        .route("/api/open-remote-editor", post(open_remote_editor))
        .route("/api/health", get(health))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", cli.port)).await?;
    tracing::info!(port = cli.port, "Desktop bridge listening");

    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

async fn open_remote_editor(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OpenRemoteEditorRequest>,
) -> impl IntoResponse {
    let signing_ctx = match SigningContext::from_jwk(req.signing_session_id, &req.private_key_jwk) {
        Ok(ctx) => ctx,
        Err(e) => {
            tracing::error!(?e, "Invalid signing context");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    // Create tunnel to the embedded SSH server on the host backend
    let local_port = match state
        .tunnel_manager
        .get_or_create_ssh_tunnel(&req.relay_session_base_url, &signing_ctx)
        .await
    {
        Ok(port) => port,
        Err(e) => {
            tracing::error!(?e, "Failed to create SSH tunnel");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    // Provision SSH key and config
    let (key_path, alias) = match ssh_config::provision_ssh_key(&signing_ctx.signing_key) {
        Ok(result) => result,
        Err(e) => {
            tracing::error!(?e, "Failed to provision SSH key");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    if let Err(e) = ssh_config::update_ssh_config(&alias, local_port, &key_path) {
        tracing::error!(?e, "Failed to update SSH config");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        );
    }

    if let Err(e) = ssh_config::ensure_ssh_include() {
        tracing::error!(?e, "Failed to ensure SSH include");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        );
    }

    let editor = req.editor_type.as_deref().unwrap_or("VS_CODE");
    let path = &req.workspace_path;

    let url = match editor.to_uppercase().as_str() {
        "ZED" => format!("zed://ssh/{alias}{path}"),
        scheme_name => {
            let scheme = match scheme_name {
                "VS_CODE_INSIDERS" => "vscode-insiders",
                "CURSOR" => "cursor",
                "WINDSURF" => "windsurf",
                "GOOGLE_ANTIGRAVITY" => "antigravity",
                _ => "vscode",
            };
            format!("{scheme}://vscode-remote/ssh-remote+{alias}{path}")
        }
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({ "url": url, "local_port": local_port, "ssh_alias": alias })),
    )
}
