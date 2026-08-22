use axum::{routing::get, Json, Router};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(|| async {
        Json(serde_json::json!({
            "platform": "MOS MicroVM",
            "runtime": "Rust Axum Native",
            "cold_start": "<10ms",
            "scale_to_zero": true
        }))
    }));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    println!("Listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
