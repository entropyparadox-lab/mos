use m_os_builder::BuilderEngine;
use std::path::PathBuf;

#[tokio::test]
async fn test_nixpacks_plan_generation() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
    let nixpacks_bin = root_dir.join("bin/nixpacks");

    let engine = BuilderEngine::new(nixpacks_bin);

    // Test planning on the mos-cli crate directory
    let cli_dir = root_dir.join("crates/mos-cli");
    let plan = engine
        .plan(&cli_dir)
        .await
        .expect("Failed to generate nixpacks plan");

    println!("Detected provider: {:?}", plan.provider);
    println!("Detected build cmds: {:?}", plan.build_cmds);
    println!("Detected start cmd: {:?}", plan.start_cmd);

    assert_eq!(plan.provider, "rust");
    assert!(plan.start_cmd.is_some());
}
