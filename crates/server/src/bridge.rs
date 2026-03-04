use std::{
    net::IpAddr,
    sync::{Arc, OnceLock},
};

use axum::{
    Json, Router,
    body::Body,
    extract::{Request, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use desktop_bridge::service::{DesktopBridgeService, OpenRemoteEditorRequest};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    validate_request::ValidateRequestHeaderLayer,
};
use url::Url;

const BRIDGE_ALLOWED_ORIGINS_ENV: &str = "VK_BRIDGE_ALLOWED_ORIGINS";
const DEFAULT_ALLOWED_ORIGINS: [&str; 3] = [
    "https://api.vibekanban.com",
    "https://api.dev.vibekanban.com",
    "https://cloud.vibekanban.com",
];

struct BridgeState {
    service: DesktopBridgeService,
}

pub fn router() -> Router {
    let state = Arc::new(BridgeState {
        service: DesktopBridgeService::default(),
    });

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::mirror_request())
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([header::CONTENT_TYPE]);

    Router::new()
        .route("/api/open-remote-editor", post(open_remote_editor))
        .route("/api/health", get(health))
        .layer(cors)
        .layer(ValidateRequestHeaderLayer::custom(validate_bridge_origin))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn open_remote_editor(
    State(state): State<Arc<BridgeState>>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct OriginKey {
    https: bool,
    host: String,
    port: u16,
}

impl OriginKey {
    fn from_origin(origin: &str) -> Option<Self> {
        let url = Url::parse(origin).ok()?;
        let https = match url.scheme() {
            "http" => false,
            "https" => true,
            _ => return None,
        };
        let host = normalize_host(url.host_str()?);
        let port = url.port_or_known_default()?;
        Some(Self { https, host, port })
    }

    fn from_host_header(host: &str, https: bool) -> Option<Self> {
        let authority: axum::http::uri::Authority = host.parse().ok()?;
        let host = normalize_host(authority.host());
        let port = authority.port_u16().unwrap_or_else(|| default_port(https));
        Some(Self { https, host, port })
    }
}

#[allow(clippy::result_large_err)]
fn validate_bridge_origin<B>(req: &mut Request<B>) -> Result<(), axum::response::Response> {
    let Some(origin) = get_origin_header(req) else {
        return Ok(());
    };

    if origin.eq_ignore_ascii_case("null") {
        return Err(forbidden());
    }

    if is_debug_loopback_origin(origin) {
        return Ok(());
    }

    let host = get_host_header(req);

    if host.is_some_and(|host| origin_matches_host(origin, host)) {
        return Ok(());
    }

    let Some(origin_key) = OriginKey::from_origin(origin) else {
        return Err(forbidden());
    };

    if allowed_origins()
        .iter()
        .any(|allowed| allowed == &origin_key)
    {
        return Ok(());
    }

    if let Some(host_key) =
        host.and_then(|host| OriginKey::from_host_header(host, origin_key.https))
        && host_key == origin_key
    {
        return Ok(());
    }

    Err(forbidden())
}

fn get_origin_header<B>(req: &Request<B>) -> Option<&str> {
    get_header(req, header::ORIGIN)
}

fn get_host_header<B>(req: &Request<B>) -> Option<&str> {
    get_header(req, header::HOST)
}

fn get_header<B>(req: &Request<B>, name: header::HeaderName) -> Option<&str> {
    req.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
}

fn forbidden() -> axum::response::Response {
    axum::response::Response::builder()
        .status(StatusCode::FORBIDDEN)
        .body(Body::empty())
        .unwrap_or_else(|_| axum::response::Response::new(Body::empty()))
}

fn origin_matches_host(origin: &str, host: &str) -> bool {
    origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .is_some_and(|rest| rest.eq_ignore_ascii_case(host))
}

fn is_debug_loopback_origin(origin: &str) -> bool {
    if !cfg!(debug_assertions) {
        return false;
    }

    let Ok(url) = Url::parse(origin) else {
        return false;
    };

    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }

    let Some(host) = url.host_str() else {
        return false;
    };

    normalize_host(host) == "localhost"
}

fn normalize_host(host: &str) -> String {
    let trimmed = host.trim().trim_start_matches('[').trim_end_matches(']');
    let lower = trimmed.to_ascii_lowercase();
    if lower == "localhost" {
        return "localhost".to_string();
    }
    if let Ok(ip) = lower.parse::<IpAddr>() {
        if ip.is_loopback() {
            return "localhost".to_string();
        }
        return ip.to_string();
    }
    lower
}

fn default_port(https: bool) -> u16 {
    if https { 443 } else { 80 }
}

fn allowed_origins() -> &'static Vec<OriginKey> {
    static ALLOWED: OnceLock<Vec<OriginKey>> = OnceLock::new();
    ALLOWED.get_or_init(|| {
        let mut origins: Vec<OriginKey> = DEFAULT_ALLOWED_ORIGINS
            .iter()
            .filter_map(|origin| OriginKey::from_origin(origin))
            .collect();

        if let Ok(value) = std::env::var(BRIDGE_ALLOWED_ORIGINS_ENV) {
            for parsed in value
                .split(',')
                .filter_map(|origin| OriginKey::from_origin(origin.trim()))
            {
                if !origins.contains(&parsed) {
                    origins.push(parsed);
                }
            }
        }

        origins
    })
}
