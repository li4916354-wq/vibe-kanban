use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use clap::Parser;
use tower_http::cors::CorsLayer;

use desktop_bridge::service::{DesktopBridgeService, OpenRemoteEditorRequest};

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
    service: DesktopBridgeService,
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
        service: DesktopBridgeService::default(),
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
    match state.service.open_remote_editor(req).await {
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
