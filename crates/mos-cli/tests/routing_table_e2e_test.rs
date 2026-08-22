use mos_edge::EdgeRouter;
use std::path::PathBuf;

fn find_mos_root() -> PathBuf {
    let mut curr = std::env::current_dir().unwrap();
    while !curr.join("crates").exists() {
        if !curr.pop() {
            break;
        }
    }
    curr
}

#[tokio::test]
async fn test_routing_table_config_loading_and_resolution() {
    let mos_root = find_mos_root();
    let example_routes_path = mos_root.join("config/routes.example.json");
    assert!(
        example_routes_path.exists(),
        "config/routes.example.json must exist"
    );

    let content = std::fs::read_to_string(&example_routes_path).unwrap();
    let router = EdgeRouter::new();
    let loaded = router
        .load_from_json(&content)
        .expect("Failed to load routes from example config");
    assert!(
        loaded >= 5,
        "Expected at least 5 example routes, got {}",
        loaded
    );

    // Verify Core Application Route resolution
    let app_route = router
        .resolve("app.example.com")
        .expect("app.example.com must resolve");
    assert_eq!(app_route.port, 8080);
    assert!(!app_route.is_suspended);

    let api_route = router
        .resolve("api.example.com")
        .expect("api.example.com must resolve");
    assert_eq!(api_route.port, 8000);
    assert!(!api_route.is_suspended);

    let auth_route = router
        .resolve("auth.example.com")
        .expect("auth.example.com must resolve");
    assert_eq!(auth_route.port, 3000);

    let docs_route = router
        .resolve("docs.example.com")
        .expect("docs.example.com must resolve");
    assert_eq!(docs_route.port, 4000);

    let worker_route = router
        .resolve("worker.example.com")
        .expect("worker.example.com must resolve");
    assert_eq!(worker_route.port, 8085);
    assert!(
        worker_route.is_suspended,
        "Worker should be marked as suspended for wake-on-HTTP test"
    );

    println!("✅ All example routing definitions parsed and verified successfully!");
}
