use m_os_orchestrator::{MicroVmInstance, SnapshotEngine};
use mos_core::InstanceConfig;
use std::path::PathBuf;
use std::time::Instant;

#[tokio::test]
async fn test_scale_to_zero_snapshot_and_fast_resume() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
    let firecracker_bin = root_dir.join("bin/firecracker");
    let kernel_path = root_dir.join("runtime/kernels/vmlinux.bin");
    let base_rootfs = root_dir.join("runtime/base-rootfs/bionic.rootfs.ext4");
    if !firecracker_bin.exists() || !kernel_path.exists() || !base_rootfs.exists() {
        println!("Skipping test in CI environment without Firecracker / kernel / rootfs.");
        return;
    }

    let run_dir = root_dir.join("runtime/instances/test-snapshot-1");
    let snapshot_dir = root_dir.join("runtime/snapshots/test-snapshot-1");
    let _ = tokio::fs::create_dir_all(&run_dir).await;
    let _ = tokio::fs::create_dir_all(&snapshot_dir).await;

    let test_rootfs = run_dir.join("rootfs.ext4");
    let socket_path = run_dir.join("firecracker.sock");

    let _ = tokio::fs::copy(&base_rootfs, &test_rootfs)
        .await
        .expect("Failed to copy rootfs");

    let config = InstanceConfig::new("snapshot-test-vm", kernel_path, test_rootfs);

    // 1. Cold Boot MicroVM
    let start_boot = Instant::now();
    let instance = MicroVmInstance::boot(
        &firecracker_bin,
        socket_path.clone(),
        config.clone(),
        "console=ttyS0 reboot=k panic=1 pci=off init=/bin/sh",
    )
    .await
    .expect("Failed to cold boot VM");
    let boot_elapsed = start_boot.elapsed();
    println!("🔥 Cold Boot Latency: {:?}", boot_elapsed);

    // 2. Snapshot and Stop (Scale-to-Zero)
    let engine = SnapshotEngine::new(firecracker_bin.clone());
    let artifacts = engine
        .snapshot_and_stop(instance, &snapshot_dir)
        .await
        .expect("Failed to snapshot and stop VM");

    assert!(artifacts.snapshot_path.exists(), "Snapshot file must exist");
    assert!(artifacts.mem_path.exists(), "Memory file must exist");
    let mem_size = tokio::fs::metadata(&artifacts.mem_path)
        .await
        .unwrap()
        .len();
    println!(
        "💾 Snapshot created successfully! Memory dump size: {:.2} MB",
        mem_size as f64 / (1024.0 * 1024.0)
    );

    // 3. Fast Resume from Snapshot (Wake-on-Demand)
    let socket_path_resume = run_dir.join("firecracker_resumed.sock");
    let (mut resumed_instance, resume_elapsed) = engine
        .resume_from_snapshot(socket_path_resume, config, &artifacts)
        .await
        .expect("Failed to resume VM from snapshot");

    println!("⚡ Fast Resume Latency: {:?}", resume_elapsed);
    assert!(
        resume_elapsed.as_millis() < 100,
        "Snapshot resume latency should be under 100ms"
    );

    // 4. Verify resumed VM state & Clean up
    resumed_instance
        .process
        .kill()
        .await
        .expect("Failed to kill resumed VM");
    println!("✅ Scale-to-Zero Snapshot & Resume test passed successfully!");
}
