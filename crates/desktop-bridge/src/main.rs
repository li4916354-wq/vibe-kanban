mod signing;
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
#[command(name = "desktop-bridge", about = "Local bridge for remote IDE opening via relay tunnel")]
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
    #[serde(default = "default_ssh_port")]
    ssh_port: u16,
    /// Relay proxy session URL (e.g. https://relay.example.com/relay/h/{host_id}/s/{session_id})
    relay_session_base_url: String,
    /// Ed25519 signing session ID
    signing_session_id: String,
    /// Ed25519 private key in JWK format
    private_key_jwk: serde_json::Value,
}

fn default_ssh_port() -> u16 {
    22
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
    let signing_ctx = match SigningContext::from_jwk(
        req.signing_session_id,
        &req.private_key_jwk,
    ) {
        Ok(ctx) => ctx,
        Err(e) => {
            tracing::error!(?e, "Invalid signing context");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let local_port = match state
        .tunnel_manager
        .get_or_create_tunnel(&req.relay_session_base_url, &signing_ctx, req.ssh_port)
        .await
    {
        Ok(port) => port,
        Err(e) => {
            tracing::error!(?e, "Failed to create tunnel");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let editor = req.editor_type.as_deref().unwrap_or("VS_CODE");
    let path = &req.workspace_path;
    let host = format!("localhost:{local_port}");

    let url = match editor.to_uppercase().as_str() {
        "ZED" => format!("zed://ssh/{host}{path}"),
        scheme_name => {
            let scheme = match scheme_name {
                "VS_CODE_INSIDERS" => "vscode-insiders",
                "CURSOR" => "cursor",
                "WINDSURF" => "windsurf",
                "GOOGLE_ANTIGRAVITY" => "antigravity",
                _ => "vscode",
            };
            format!("{scheme}://vscode-remote/ssh-remote+{host}{path}")
        }
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({ "url": url, "local_port": local_port })),
    )
}
