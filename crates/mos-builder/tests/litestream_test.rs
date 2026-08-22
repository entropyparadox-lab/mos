use m_os_builder::litestream::LitestreamManager;
use std::fs;
use std::path::PathBuf;

#[test]
fn test_litestream_sqlite_detection() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let db_path = temp_dir.path().join("app.db");
    fs::write(&db_path, "SQLite format 3\0").expect("Failed to write mock db");

    let detected = LitestreamManager::detect_sqlite(temp_dir.path());
    assert!(detected.is_some());
    assert_eq!(detected.unwrap(), db_path);
}

#[test]
fn test_litestream_yaml_generation() {
    let yaml = LitestreamManager::generate_litestream_yaml(
        "inst-abc-123",
        &PathBuf::from("/app/data/app.db"),
        Some("my-bucket"),
        Some("https://account.r2.cloudflarestorage.com"),
    );

    assert!(yaml.contains("/app/data/app.db"));
    assert!(yaml.contains("my-bucket"));
    assert!(yaml.contains("instances/inst-abc-123/app.db"));
    assert!(yaml.contains("https://account.r2.cloudflarestorage.com"));

    let restore_cmd = LitestreamManager::generate_restore_command(
        &PathBuf::from("/etc/litestream.yml"),
        &PathBuf::from("/app/data/app.db"),
    );
    assert!(restore_cmd.contains("litestream restore -if-replica-exists"));
}
