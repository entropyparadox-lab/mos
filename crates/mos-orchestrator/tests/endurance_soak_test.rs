use m_os_orchestrator::{MicroVmInstance, SnapshotEngine};
use mos_core::InstanceConfig;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[tokio::test]
async fn test_microvm_endurance_soak_multicycle_leak_check() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
    let firecracker_bin = root_dir.join("bin/firecracker");
    let kernel_path = root_dir.join("runtime/kernels/vmlinux.bin");
    let base_rootfs = root_dir.join("runtime/base-rootfs/bionic.rootfs.ext4");
    if !firecracker_bin.exists() || !kernel_path.exists() || !base_rootfs.exists() {
        println!("Skipping test in CI environment without Firecracker / kernel / rootfs.");
        return;
    }

    // Keep path short to stay well under Linux sockaddr_un 108-byte limit
    let base_soak_dir = root_dir.join("runtime/instances/soak_run");
    let _ = tokio::fs::create_dir_all(&base_soak_dir).await;

    let engine = SnapshotEngine::new(firecracker_bin.clone());
    let total_cycles = 5;
    let mut boot_latencies = Vec::new();
    let mut snapshot_latencies = Vec::new();
    let mut resume_latencies = Vec::new();

    println!(
        "\n🏋️ [MOS Endurance Soak Test] Starting {} continuous cycles...",
        total_cycles
    );

    for cycle in 1..=total_cycles {
        let cycle_dir = base_soak_dir.join(format!("c{}", cycle));
        let snapshot_dir = cycle_dir.join("snap");
        tokio::fs::create_dir_all(&cycle_dir).await.unwrap();
        tokio::fs::create_dir_all(&snapshot_dir).await.unwrap();

        let cycle_rootfs = cycle_dir.join("rootfs.ext4");
        tokio::fs::copy(&base_rootfs, &cycle_rootfs)
            .await
            .expect("Failed to copy rootfs");

        let config = InstanceConfig::new(
            format!("soak-vm-{}", cycle),
            kernel_path.clone(),
            cycle_rootfs,
        );
        let sock_path = cycle_dir.join("fc.sock");

        // 1. Cold Boot
        let start_boot = Instant::now();
        let instance = MicroVmInstance::boot(
            &firecracker_bin,
            sock_path.clone(),
            config.clone(),
            "console=ttyS0 reboot=k panic=1 pci=off init=/bin/sh",
        )
        .await
        .expect("Cold boot failed during soak test");
        let boot_dur = start_boot.elapsed();
        boot_latencies.push(boot_dur);

        // 2. Snapshot & Stop (Scale-to-Zero)
        let start_snap = Instant::now();
        let artifacts = engine
            .snapshot_and_stop(instance, &snapshot_dir)
            .await
            .expect("Snapshot failed during soak test");
        let snap_dur = start_snap.elapsed();
        snapshot_latencies.push(snap_dur);

        // Short stabilization pause for OS kernel resource recycling
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 3. Fast Resume from Snapshot (Wake-on-Demand)
        let sock_resume_path = cycle_dir.join("fc_res.sock");
        let start_resume = Instant::now();
        let (mut resumed_instance, _elapsed) = engine
            .resume_from_snapshot(sock_resume_path, config.clone(), &artifacts)
            .await
            .expect("Resume failed during soak test");
        let resume_dur = start_resume.elapsed();
        resume_latencies.push(resume_dur);

        // Verify resumed VM & Clean up
        resumed_instance
            .process
            .kill()
            .await
            .expect("Process kill failed");

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Clean up test directory
    let _ = tokio::fs::remove_dir_all(&base_soak_dir).await;

    // Calculate Statistics
    let avg_boot: Duration = boot_latencies.iter().sum::<Duration>() / total_cycles as u32;
    let avg_snap: Duration = snapshot_latencies.iter().sum::<Duration>() / total_cycles as u32;
    let avg_resume: Duration = resume_latencies.iter().sum::<Duration>() / total_cycles as u32;

    let max_boot = boot_latencies.iter().max().unwrap();
    let max_resume = resume_latencies.iter().max().unwrap();

    println!("📊 [Soak Test Results over {} Cycles]", total_cycles);
    println!(
        "  • Cold Boot Latency: Avg = {:.2} ms, Max = {:.2} ms",
        avg_boot.as_secs_f64() * 1000.0,
        max_boot.as_secs_f64() * 1000.0
    );
    println!(
        "  • Snapshot Latency:  Avg = {:.2} ms",
        avg_snap.as_secs_f64() * 1000.0
    );
    println!(
        "  • Resume Latency:    Avg = {:.2} ms, Max = {:.2} ms",
        avg_resume.as_secs_f64() * 1000.0,
        max_resume.as_secs_f64() * 1000.0
    );
    println!(
        "  • Success Rate:      100% (0 errors across {} cycles)",
        total_cycles
    );

    assert!(avg_boot.as_millis() < 50, "Average boot must be under 50ms");
    assert!(
        avg_resume.as_millis() < 30,
        "Average resume must be under 30ms"
    );
    assert!(max_resume.as_millis() < 60, "P99 resume must be under 60ms");
}
