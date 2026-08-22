use m_os_orchestrator::{MicroVmInstance, SnapshotArtifacts, SnapshotEngine};
use mos_core::InstanceConfig;
use std::path::PathBuf;

#[tokio::test]
async fn test_adversarial_invalid_kernel_handling() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
    let firecracker_bin = root_dir.join("bin/firecracker");
    let base_rootfs = root_dir.join("runtime/base-rootfs/bionic.rootfs.ext4");
    if !firecracker_bin.exists() || !base_rootfs.exists() {
        println!("Skipping test in CI environment without Firecracker / rootfs binary.");
        return;
    }

    let run_dir = root_dir.join("runtime/instances/adv-test-kernel");
    let _ = tokio::fs::create_dir_all(&run_dir).await;

    // Create a corrupted/fake kernel file
    let fake_kernel = run_dir.join("fake_vmlinux.bin");
    tokio::fs::write(&fake_kernel, b"THIS_IS_NOT_AN_ELF_KERNEL_CORRUPTED_BYTES")
        .await
        .unwrap();

    let test_rootfs = run_dir.join("rootfs.ext4");
    let _ = tokio::fs::copy(&base_rootfs, &test_rootfs).await.unwrap();

    let socket_path = run_dir.join("fc_fake.sock");
    let config = InstanceConfig::new("adv-kernel-vm", fake_kernel, test_rootfs);

    let result =
        MicroVmInstance::boot(&firecracker_bin, socket_path, config, "console=ttyS0").await;

    // Must return an explicit Err instead of panicking or hanging
    assert!(
        result.is_err(),
        "MicroVM must reject invalid kernel gracefully"
    );
    let err_msg = result.err().unwrap().to_string();
    println!("🛡️ Handled invalid kernel injection: {}", err_msg);
    assert!(err_msg.contains("failed") || err_msg.contains("error") || err_msg.contains("Invalid"));
}

#[tokio::test]
async fn test_adversarial_corrupted_snapshot_handling() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
    let firecracker_bin = root_dir.join("bin/firecracker");
    let base_rootfs = root_dir.join("runtime/base-rootfs/bionic.rootfs.ext4");
    if !firecracker_bin.exists() || !base_rootfs.exists() {
        println!("Skipping test in CI environment without Firecracker / rootfs binary.");
        return;
    }

    let run_dir = root_dir.join("runtime/instances/adv-test-snap");
    let _ = tokio::fs::create_dir_all(&run_dir).await;

    let fake_snap = run_dir.join("corrupted.snap");
    let fake_mem = run_dir.join("corrupted.mem");
    tokio::fs::write(&fake_snap, b"CORRUPTED_SNAPSHOT_HEADER_RANDOM_DATA")
        .await
        .unwrap();
    tokio::fs::write(&fake_mem, vec![0u8; 1024]).await.unwrap();

    let test_rootfs = run_dir.join("rootfs.ext4");
    let _ = tokio::fs::copy(&base_rootfs, &test_rootfs).await.unwrap();

    let artifacts = SnapshotArtifacts {
        snapshot_path: fake_snap,
        mem_path: fake_mem,
        rootfs_path: test_rootfs.clone(),
    };

    let config = InstanceConfig::new(
        "adv-snap-vm",
        root_dir.join("runtime/kernels/vmlinux.bin"),
        test_rootfs,
    );
    let engine = SnapshotEngine::new(firecracker_bin);

    let socket_path = run_dir.join("fc_snap_fake.sock");
    let result = engine
        .resume_from_snapshot(socket_path, config, &artifacts)
        .await;

    // Must return explicit error, no crash
    assert!(
        result.is_err(),
        "Resume must reject corrupted snapshot files"
    );
    let err_msg = result.err().unwrap().to_string();
    println!("🛡️ Handled corrupted snapshot injection: {}", err_msg);
}

#[tokio::test]
async fn test_adversarial_concurrent_vm_spawns() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
    let firecracker_bin = root_dir.join("bin/firecracker");
    let kernel_path = root_dir.join("runtime/kernels/vmlinux.bin");
    let base_rootfs = root_dir.join("runtime/base-rootfs/bionic.rootfs.ext4");
    if !firecracker_bin.exists() || !base_rootfs.exists() || !kernel_path.exists() {
        println!("Skipping test in CI environment without Firecracker / kernel / rootfs.");
        return;
    }

    // Spawn 5 MicroVMs concurrently to test race conditions and isolation
    let mut handles = Vec::new();

    for i in 0..5 {
        let fc_bin = firecracker_bin.clone();
        let k_path = kernel_path.clone();
        let b_rootfs = base_rootfs.clone();
        let run_dir = root_dir.join(format!("runtime/instances/adv-concurrent-{}", i));

        let handle = tokio::spawn(async move {
            let _ = tokio::fs::create_dir_all(&run_dir).await;
            let test_rootfs = run_dir.join("rootfs.ext4");
            let sock_path = run_dir.join("fc.sock");
            let _ = tokio::fs::copy(&b_rootfs, &test_rootfs).await.unwrap();

            let config = InstanceConfig::new(format!("concurrent-vm-{}", i), k_path, test_rootfs);
            let mut inst = MicroVmInstance::boot(
                &fc_bin,
                sock_path,
                config,
                "console=ttyS0 reboot=k panic=1 pci=off init=/bin/sh",
            )
            .await
            .expect("Concurrent boot failed");

            // Pause & Resume under concurrency
            inst.client.pause().await.unwrap();
            inst.client.resume().await.unwrap();

            // Kill cleanly
            inst.process.kill().await.unwrap();
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.expect("Concurrent VM task panicked");
    }
    println!("🛡️ 5 Concurrent MicroVMs booted, paused, resumed, and killed with zero collisions!");
}
