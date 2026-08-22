use mos_builder::{BuilderEngine, LitestreamManager};
use mos_core::InstanceId;
use mos_edge::{EdgeRouter, RouteTarget};
use std::path::PathBuf;

#[tokio::test]
async fn test_e2e_multi_app_build_and_routing_verification() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir.parent().unwrap().parent().unwrap();
    let nixpacks_bin = project_root.join("bin/nixpacks");

    let builder = BuilderEngine::new(nixpacks_bin);

    // 1. Next.js App Planning
    let nextjs_dir = project_root.join("examples/vibe-nextjs-app");
    let nextjs_plan = builder
        .plan(&nextjs_dir)
        .await
        .expect("Failed to plan Next.js");
    assert_eq!(nextjs_plan.provider, "node");
    assert!(nextjs_plan.start_cmd.is_some());
    assert!(nextjs_plan
        .start_cmd
        .as_ref()
        .unwrap()
        .contains("npm run start"));

    // 2. FastAPI + SQLite Planning & Litestream Detection
    let fastapi_dir = project_root.join("examples/vibe-fastapi-app");
    let fastapi_plan = builder
        .plan(&fastapi_dir)
        .await
        .expect("Failed to plan FastAPI");
    assert_eq!(fastapi_plan.provider, "python");
    assert!(fastapi_plan.start_cmd.is_some());

    let _detected_sqlite = LitestreamManager::detect_sqlite(&fastapi_dir);
    let litestream_yaml = LitestreamManager::generate_litestream_yaml(
        "inst-fastapi-01",
        &fastapi_dir.join("app.db"),
        Some("mos-bucket"),
        Some("https://r2.mos.dev"),
    );
    assert!(litestream_yaml.contains("inst-fastapi-01"));

    // 3. Rust Axum App Planning
    let axum_dir = project_root.join("examples/vibe-axum-app");
    let axum_plan = builder.plan(&axum_dir).await.expect("Failed to plan Axum");
    assert_eq!(axum_plan.provider, "rust");

    // 4. Subdomain Router Integration with Wake-on-HTTP Target
    let router = EdgeRouter::new();
    let nextjs_id = InstanceId::new();
    let fastapi_id = InstanceId::new();
    let axum_id = InstanceId::new();

    router.register(
        "nextjs.mos.local",
        RouteTarget::new(nextjs_id, "127.0.0.1", 8081, false),
    );
    router.register(
        "fastapi.mos.local",
        RouteTarget::new(fastapi_id, "127.0.0.1", 8082, true),
    );
    router.register(
        "axum.mos.local",
        RouteTarget::new(axum_id, "127.0.0.1", 8083, false),
    );

    assert_eq!(
        router.resolve("nextjs.mos.local").map(|t| t.is_suspended),
        Some(false)
    );
    assert_eq!(
        router.resolve("fastapi.mos.local").map(|t| t.is_suspended),
        Some(true)
    );
    assert_eq!(
        router.resolve("axum.mos.local").map(|t| t.is_suspended),
        Some(false)
    );
}
