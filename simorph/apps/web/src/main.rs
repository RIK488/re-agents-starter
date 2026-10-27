//! SIMORPH web server: HTTP API + single-page interface.

use simorph_web::{build_app, AppState};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    simorph_telemetry::init(&simorph_telemetry::TelemetryConfig::default());
    let data = std::env::var("SIMORPH_DATA").unwrap_or_else(|_| ".simorph".into());
    let bind = std::env::var("SIMORPH_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let state = Arc::new(AppState::open(&data).expect("workspace"));
    let app = build_app(state);
    let listener = tokio::net::TcpListener::bind(&bind).await.expect("bind");
    tracing::info!(%bind, data, "SIMORPH web interface ready");
    println!("SIMORPH : http://{bind}  (données : {data})");
    axum::serve(listener, app).await.expect("serve");
}
