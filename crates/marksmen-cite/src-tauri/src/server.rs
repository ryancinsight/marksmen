use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tauri::{AppHandle, Emitter};
use tower_http::cors::{Any, CorsLayer};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ImportPayload {
    pub title: Option<String>,
    pub authors: Option<Vec<String>>,
    pub abstract_text: Option<String>,
    pub journal: Option<String>,
    pub year: Option<String>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub source: Option<String>,
}

pub async fn start_server(app: AppHandle) {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app_router = Router::new()
        .route("/import", post(handle_import))
        .layer(cors)
        .with_state(app);

    let addr = SocketAddr::from(([127, 0, 0, 1], 14242));

    // Spawn server in background
    tokio::spawn(async move {
        if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
            let _ = axum::serve(listener, app_router).await;
        }
    });
}

async fn handle_import(
    State(app): State<AppHandle>,
    Json(payload): Json<ImportPayload>,
) -> impl IntoResponse {
    // Emit event to the frontend UI
    if let Err(e) = app.emit("web-import", payload) {
        eprintln!("Failed to emit web-import event: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to emit event");
    }

    (StatusCode::OK, "Imported successfully")
}
