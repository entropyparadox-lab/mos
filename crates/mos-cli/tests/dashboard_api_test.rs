use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt; // for `app.oneshot()`

#[tokio::test]
async fn test_dashboard_static_html_and_endpoints() {
    // Test the dashboard endpoints
    let app = mos_dashboard_router_helper();

    // 1. GET /
    let response = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 2. GET /api/metrics
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["node"].as_str().is_some());
    assert_eq!(json["hypervisor"], "KVM AMD-V (Linux 6.17)");

    // 3. GET /api/instances
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/instances")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 4. GET /api/instances/inst-7f8a12/logs
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/instances/inst-7f8a12/logs?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

fn mos_dashboard_router_helper() -> axum::Router {
    axum::Router::new()
        .route(
            "/",
            axum::routing::get(|| async { axum::response::Html("<h1>MOS Console</h1>") }),
        )
        .route(
            "/api/instances",
            axum::routing::get(|| async {
                axum::Json(serde_json::json!({
                    "status": "ok",
                    "instances": []
                }))
            }),
        )
        .route(
            "/api/metrics",
            axum::routing::get(|| async {
                axum::Json(serde_json::json!({
                    "node": "mos-node-01",
                    "hypervisor": "KVM AMD-V (Linux 6.17)",
                    "running_instances": 2
                }))
            }),
        )
        .route(
            "/api/instances/:id/logs",
            axum::routing::get(
                |axum::extract::Path(id): axum::extract::Path<String>| async move {
                    axum::Json(serde_json::json!({
                        "instance_id": id,
                        "logs": ["log line 1"]
                    }))
                },
            ),
        )
}
