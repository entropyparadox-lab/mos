use m_os_orchestrator::MicroVmInstance;
use mos_core::InstanceConfig;
use std::path::PathBuf;
use std::time::Instant;

#[tokio::test]
async fn test_microvm_boot_lifecycle() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
    let firecracker_bin = root_dir.join("bin/firecracker");
    let kernel_path = root_dir.join("runtime/kernels/vmlinux.bin");
    let base_rootfs = root_dir.join("runtime/base-rootfs/bionic.rootfs.ext4");
    if !firecracker_bin.exists() || !kernel_path.exists() || !base_rootfs.exists() {
        println!("Skipping test in CI environment without Firecracker / kernel / rootfs.");
        return;
    }

    let run_dir = root_dir.join("runtime/instances/test-integration-1");
    let _ = tokio::fs::create_dir_all(&run_dir).await;
    let test_rootfs = run_dir.join("rootfs.ext4");
    let socket_path = run_dir.join("firecracker.sock");

    // Copy clean rootfs
    let _ = tokio::fs::copy(&base_rootfs, &test_rootfs)
        .await
        .expect("Failed to copy rootfs");

    let config = InstanceConfig::new("integration-test-vm", kernel_path, test_rootfs);

    let start = Instant::now();
    let mut instance = MicroVmInstance::boot(
        &firecracker_bin,
        socket_path,
        config,
        "console=ttyS0 reboot=k panic=1 pci=off init=/bin/sh",
    )
    .await
    .expect("Failed to boot MicroVM instance");
    let elapsed = start.elapsed();

    println!(
        "MicroVM booted successfully via Rust Orchestrator in {:?}",
        elapsed
    );
    assert!(
        elapsed.as_millis() < 500,
        "MicroVM boot time should be under 500ms"
    );

    // Test pause and resume
    instance.client.pause().await.expect("Failed to pause VM");
    instance.client.resume().await.expect("Failed to resume VM");

    // Clean up
    instance
        .process
        .kill()
        .await
        .expect("Failed to kill VM process");
}
