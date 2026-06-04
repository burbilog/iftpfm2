use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Json},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use base64::Engine as _;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

use iftpfm2::{Config, ConfigEntry, parse_config_entries, write_config_entries};

// ── CLI args ──────────────────────────────────────────────────────────

struct AppArgs {
    config_path: String,
    listen: String,
    readonly: bool,
    user: Option<String>,
    password: Option<String>,
}

fn parse_args() -> AppArgs {
    let args: Vec<String> = env::args().collect();
    let mut config_path = None;
    let mut listen = "127.0.0.1:3000".to_string();
    let mut readonly = false;
    let mut user = None;
    let mut password = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                config_path = Some(args[i].clone());
            }
            "--listen" => {
                i += 1;
                listen = args[i].clone();
            }
            "--readonly" => {
                readonly = true;
            }
            "--user" => {
                i += 1;
                user = Some(args[i].clone());
            }
            "--password" => {
                i += 1;
                password = Some(args[i].clone());
            }
            "-h" | "--help" => {
                eprintln!("iftpfm2-web — web UI for iftpfm2 config editing\n\n\
                    Usage: iftpfm2-web --config <path> [options]\n\n\
                    Options:\n  \
                      --config <path>     Path to JSONL config file (required)\n  \
                      --listen <addr>     Listen address:port (default: 127.0.0.1:3000)\n  \
                      --readonly          Read-only mode (disables write operations)\n  \
                      --user <login>      Basic Auth username (env: IFTPFM2_WEB_USER)\n  \
                      --password <pass>   Basic Auth password (env: IFTPFM2_WEB_PASSWORD)\n  \
                      -h, --help          Show this help message");
                std::process::exit(0);
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    // Env fallbacks
    if user.is_none() {
        user = env::var("IFTPM2_WEB_USER").ok()
            .or_else(|| env::var("IFTPFM2_WEB_USER").ok());
    }
    if password.is_none() {
        password = env::var("IFTPM2_WEB_PASSWORD").ok()
            .or_else(|| env::var("IFTPFM2_WEB_PASSWORD").ok());
    }

    let config_path = config_path.unwrap_or_else(|| {
        eprintln!("Error: --config <path> is required");
        std::process::exit(1);
    });

    AppArgs { config_path, listen, readonly, user, password }
}

// ── App state ─────────────────────────────────────────────────────────

struct AppState {
    config_path: String,
    readonly: bool,
    auth_user: Option<String>,
    auth_password: Option<String>,
    entries: Mutex<Vec<ConfigEntry>>,
}

// ── Basic Auth middleware ─────────────────────────────────────────────

fn check_auth(headers: &HeaderMap, state: &AppState) -> Result<(), StatusCode> {
    let (expected_user, expected_pass) = match (&state.auth_user, &state.auth_password) {
        (Some(u), Some(p)) => (u, p),
        _ => return Ok(()), // No auth configured
    };

    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !auth_header.starts_with("Basic ") {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let encoded = &auth_header[6..];
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap_or_default();

    let credentials = String::from_utf8_lossy(&decoded);
    let mut parts = credentials.splitn(2, ':');
    let user = parts.next().unwrap_or("");
    let pass = parts.next().unwrap_or("");

    if user != expected_user {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if !constant_time_eq::constant_time_eq(pass.as_bytes(), expected_pass.as_bytes()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(())
}

// ── JSON wrapper for ConfigEntry (adds index) ─────────────────────────

#[derive(Serialize)]
struct ConfigEntryResponse {
    index: usize,
    comment: String,
    config: serde_json::Value,
}

fn entry_to_response(index: usize, entry: &ConfigEntry) -> ConfigEntryResponse {
    ConfigEntryResponse {
        index,
        comment: entry.comment.clone(),
        config: serde_json::to_value(&entry.config).unwrap_or_default(),
    }
}

// ── API handlers ──────────────────────────────────────────────────────

async fn get_configs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(code) = check_auth(&headers, &state) {
        return (code, Json(json!({"error": "Unauthorized"}))).into_response();
    }

    let entries = state.entries.lock().await;
    let result: Vec<ConfigEntryResponse> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| entry_to_response(i, e))
        .collect();

    Json(json!(result)).into_response()
}

async fn get_config(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(code) = check_auth(&headers, &state) {
        return (code, Json(json!({"error": "Unauthorized"}))).into_response();
    }

    let entries = state.entries.lock().await;
    match entries.get(index) {
        Some(entry) => Json(json!(entry_to_response(index, entry))).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "Index out of range"}))).into_response(),
    }
}

#[derive(Deserialize)]
struct CreateConfigRequest {
    comment: Option<String>,
    config: serde_json::Value,
}

async fn create_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateConfigRequest>,
) -> impl IntoResponse {
    if let Err(code) = check_auth(&headers, &state) {
        return (code, Json(json!({"error": "Unauthorized"}))).into_response();
    }
    if state.readonly {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "Read-only mode"}))).into_response();
    }

    let config: Config = match serde_json::from_value(body.config) {
        Ok(c) => c,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Invalid config: {}", e)}))).into_response(),
    };

    if let Err(e) = config.validate_for_edit() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Validation failed: {}", e)}))).into_response();
    }

    let comment = body.comment.unwrap_or_default();
    let entry = ConfigEntry { comment, config };

    let mut entries = state.entries.lock().await;
    let index = entries.len();
    entries.push(entry);

    if let Err(e) = write_config_entries(&state.config_path, &entries) {
        entries.pop();
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Write failed: {}", e)}))).into_response();
    }

    (StatusCode::CREATED, Json(json!({"index": index}))).into_response()
}

async fn update_config(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
    headers: HeaderMap,
    Json(body): Json<CreateConfigRequest>,
) -> impl IntoResponse {
    if let Err(code) = check_auth(&headers, &state) {
        return (code, Json(json!({"error": "Unauthorized"}))).into_response();
    }
    if state.readonly {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "Read-only mode"}))).into_response();
    }

    let config: Config = match serde_json::from_value(body.config) {
        Ok(c) => c,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Invalid config: {}", e)}))).into_response(),
    };

    if let Err(e) = config.validate_for_edit() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Validation failed: {}", e)}))).into_response();
    }

    let comment = body.comment.unwrap_or_default();
    let new_entry = ConfigEntry { comment, config };

    let mut entries = state.entries.lock().await;
    if index >= entries.len() {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "Index out of range"}))).into_response();
    }

    let old_entry = std::mem::replace(&mut entries[index], new_entry);

    if let Err(e) = write_config_entries(&state.config_path, &entries) {
        entries[index] = old_entry;
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Write failed: {}", e)}))).into_response();
    }

    (StatusCode::OK, Json(json!({"index": index}))).into_response()
}

async fn delete_config(
    State(state): State<Arc<AppState>>,
    Path(index): Path<usize>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(code) = check_auth(&headers, &state) {
        return (code, Json(json!({"error": "Unauthorized"}))).into_response();
    }
    if state.readonly {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "Read-only mode"}))).into_response();
    }

    let mut entries = state.entries.lock().await;
    if index >= entries.len() {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "Index out of range"}))).into_response();
    }

    let removed = entries.remove(index);

    if let Err(e) = write_config_entries(&state.config_path, &entries) {
        entries.insert(index, removed);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Write failed: {}", e)}))).into_response();
    }

    (StatusCode::OK, Json(json!({"deleted": index}))).into_response()
}

async fn validate_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateConfigRequest>,
) -> impl IntoResponse {
    if let Err(code) = check_auth(&headers, &state) {
        return (code, Json(json!({"error": "Unauthorized"}))).into_response();
    }

    let config: Config = match serde_json::from_value(body.config) {
        Ok(c) => c,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": format!("Invalid config: {}", e)}))).into_response(),
    };

    match config.validate_for_edit() {
        Ok(()) => (StatusCode::OK, Json(json!({"valid": true}))).into_response(),
        Err(e) => (StatusCode::OK, Json(json!({"valid": false, "error": format!("{}", e)}))).into_response(),
    }
}

// ── SPA frontend ──────────────────────────────────────────────────────

async fn serve_index(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(_) = check_auth(&headers, &state) {
        return (
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                header::HeaderValue::from_static(r#"Basic realm="iftpfm2""#),
            )],
            Html(""),
        ).into_response();
    }
    Html(INDEX_HTML).into_response()
}

const INDEX_HTML: &str = include_str!("../static/index.html");

// ── Main ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let args = parse_args();

    eprintln!("Loading config from: {}", args.config_path);

    let entries = parse_config_entries(&args.config_path).unwrap_or_else(|e| {
        eprintln!("Error loading config: {}", e);
        std::process::exit(1);
    });
    eprintln!("Loaded {} config entries", entries.len());

    let readonly = args.readonly;
    if readonly {
        eprintln!("Running in read-only mode");
    }

    if args.user.is_some() {
        eprintln!("Basic Auth enabled (user: {})", args.user.as_deref().unwrap_or(""));
    } else {
        eprintln!("Warning: No authentication configured — anyone can access the UI");
    }

    let state = Arc::new(AppState {
        config_path: args.config_path,
        readonly,
        auth_user: args.user,
        auth_password: args.password,
        entries: Mutex::new(entries),
    });

    let api_routes = Router::new()
        .route("/api/configs", get(get_configs).post(create_config))
        .route("/api/configs/{index}", get(get_config).put(update_config).delete(delete_config))
        .route("/api/validate", post(validate_config));

    let app = Router::new()
        .route("/", get(serve_index))
        .merge(api_routes)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&args.listen).await.unwrap_or_else(|e| {
        eprintln!("Error binding to {}: {}", args.listen, e);
        std::process::exit(1);
    });

    eprintln!("iftpfm2-web listening on http://{}", args.listen);
    axum::serve(listener, app).await.unwrap_or_else(|e| {
        eprintln!("Server error: {}", e);
        std::process::exit(1);
    });
}
