use crate::browser;
use crate::desktop;
use crate::pulse::{Pulse, SILENT_LABEL, SILENT_SINK};
use anyhow::Result;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tracing::warn;

#[derive(Clone)]
pub struct AppState {
    pub pulse: Arc<Mutex<Pulse>>,
    pub listen_url: String,
    pub web_root: PathBuf,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Serialize)]
struct StatusBody {
    ok: bool,
    desktop: String,
    session: String,
    tips: Vec<String>,
    silent_sink: &'static str,
    silent_sink_ready: bool,
    silent_sink_label: &'static str,
    default_sink: String,
    default_monitor: String,
    audio_running: bool,
    capture_source: Option<String>,
    routing_mode: String,
    browsers: Vec<String>,
    chosen_browser: Option<String>,
    listen: String,
}

#[derive(Deserialize, Default)]
struct StartBody {
    source: Option<String>,
    app_indices: Option<Vec<u32>>,
    exclude_browser: Option<bool>,
    loopback: Option<bool>,
}

pub fn router(state: AppState) -> Router {
    let index = state.web_root.join("index.html");
    let static_files = ServeDir::new(&state.web_root).not_found_service(ServeFile::new(index));

    Router::new()
        .route("/api/status", get(status))
        .route("/api/sources", get(sources))
        .route("/api/apps", get(apps))
        .route("/api/audio/start", post(audio_start))
        .route("/api/audio/stop", post(audio_stop))
        .route("/api/open-browser", post(open_browser))
        .route("/ws/audio", get(ws_audio))
        .fallback_service(static_files)
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any))
        .with_state(state)
}

async fn status(State(state): State<AppState>) -> impl IntoResponse {
    let pulse = state.pulse.lock().await;
    let desktop = desktop::desktop_name();
    Json(StatusBody {
        ok: true,
        tips: desktop::tips(&desktop),
        session: desktop::session_type(),
        desktop,
        silent_sink: SILENT_SINK,
        silent_sink_ready: pulse.silent_ready(),
        silent_sink_label: SILENT_LABEL,
        default_sink: Pulse::default_sink().unwrap_or_default(),
        default_monitor: Pulse::default_monitor().unwrap_or_default(),
        audio_running: pulse.capture_source.is_some(),
        capture_source: pulse.capture_source.clone(),
        routing_mode: pulse.routing_mode.clone(),
        browsers: browser::available_browsers(),
        chosen_browser: browser::chosen_browser(),
        listen: state.listen_url.clone(),
    })
}

async fn sources() -> impl IntoResponse {
    match Pulse::list_sources() {
        Ok(list) => Json(list).into_response(),
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

async fn apps() -> impl IntoResponse {
    match Pulse::list_apps() {
        Ok(list) => Json(list).into_response(),
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

async fn audio_start(State(state): State<AppState>, Json(body): Json<StartBody>) -> impl IntoResponse {
    let mut pulse = state.pulse.lock().await;
    match pulse
        .start(
            body.source,
            body.app_indices.unwrap_or_default(),
            body.exclude_browser.unwrap_or(false),
            body.loopback.unwrap_or(true),
        )
        .await
    {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

async fn audio_stop(State(state): State<AppState>) -> impl IntoResponse {
    let mut pulse = state.pulse.lock().await;
    match pulse.stop().await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

async fn open_browser(State(state): State<AppState>) -> impl IntoResponse {
    match browser::open_in_chromium(&state.listen_url) {
        Ok(bin) => Json(serde_json::json!({ "ok": true, "browser": bin })).into_response(),
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}

async fn ws_audio(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    {
        let mut pulse = state.pulse.lock().await;
        if pulse.capture_source.is_none() {
            if let Err(err) = pulse.start(None, Vec::new(), false, true).await {
                warn!("auto-start capture failed: {err}");
                let _ = socket
                    .send(Message::Text(format!("error: {err}").into()))
                    .await;
                return;
            }
        }
    }

    let mut rx = {
        let pulse = state.pulse.lock().await;
        pulse.tx.subscribe()
    };

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            frame = rx.recv() => {
                match frame {
                    Ok(bytes) => {
                        if socket.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
        }
    }
}

fn json_error(status: StatusCode, err: impl std::fmt::Display) -> axum::response::Response {
    (status, Json(ErrorBody { error: err.to_string() })).into_response()
}

pub fn find_web_root() -> Result<PathBuf> {
    let mut starts = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            starts.push(dir.to_path_buf());
        }
    }
    for mut dir in starts {
        for _ in 0..8 {
            let candidate = dir.join("web/build");
            if candidate.join("index.html").is_file() {
                return Ok(candidate);
            }
            if !dir.pop() {
                break;
            }
        }
    }
    anyhow::bail!("web UI not found; run: cd web && npm run build")
}
