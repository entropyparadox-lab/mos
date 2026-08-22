use m_os_orchestrator::{SnapshotArtifacts, UffdSnapshotEngine};
use std::fs;
use std::path::PathBuf;

#[test]
fn test_zstd_snapshot_compression_and_decompression() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let out_dir = temp_dir.path().join("compressed");
    let decompress_dir = temp_dir.path().join("decompressed");
    fs::create_dir_all(&decompress_dir).unwrap();

    // 1. Create a simulated 1MB sparse memory snapshot
    let dummy_snap = temp_dir.path().join("vm.snap");
    let dummy_mem = temp_dir.path().join("vm.mem");
    let dummy_rootfs = temp_dir.path().join("rootfs.ext4");
    fs::write(&dummy_snap, "FIRECRACKER_SNAPSHOT_HEADER_V1").unwrap();
    fs::write(&dummy_rootfs, "EXT4_DUMMY_DISK").unwrap();

    // Create a 1MB memory file with repeated page data (compressible)
    let page = vec![0x42u8; 4096];
    let mut mem_data = Vec::new();
    for _ in 0..256 {
        mem_data.extend_from_slice(&page);
    }
    fs::write(&dummy_mem, &mem_data).unwrap();

    let artifacts = SnapshotArtifacts {
        snapshot_path: dummy_snap,
        mem_path: dummy_mem,
        rootfs_path: dummy_rootfs,
    };

    let engine = UffdSnapshotEngine::new(PathBuf::from("/usr/bin/firecracker"));

    // 2. Test ZSTD compression
    let compressed = engine
        .compress_snapshot(&artifacts, &out_dir)
        .expect("Failed to compress");

    assert!(compressed.compressed_size_bytes < compressed.original_size_bytes);
    assert!(compressed.compression_ratio < 0.1); // Repeated data should compress > 90%
    assert!(compressed.mem_zstd_path.exists());

    // 3. Test ZSTD decompression
    let restored_mem = decompress_dir.join("restored.mem");
    let restored_artifacts = engine
        .decompress_snapshot(&compressed, &restored_mem)
        .expect("Failed to decompress");

    assert_eq!(restored_artifacts.mem_path, restored_mem);
    let restored_data = fs::read(&restored_mem).unwrap();
    assert_eq!(restored_data.len(), mem_data.len());
    assert_eq!(restored_data, mem_data);
}
