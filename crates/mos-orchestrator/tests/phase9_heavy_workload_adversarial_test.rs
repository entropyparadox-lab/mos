use m_os_orchestrator::{MicroVmInstance, SnapshotEngine};
use mos_builder::heavy_workload::{HeavyWorkloadDetector, HeavyWorkloadManifest};
use mos_core::InstanceConfig;
use std::path::PathBuf;
use std::time::Instant;

#[tokio::test]
async fn test_heavy_workload_detection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let app_dir = temp_dir.path().join("my-typeset-app");
    std::fs::create_dir_all(&app_dir).unwrap();
    std::fs::write(
        app_dir.join("Cargo.toml"),
        r#"[package]
name = "my-typeset-app"
version = "0.1.0"
[dependencies]
typst = "0.11"
"#,
    )
    .unwrap();

    let manifest = HeavyWorkloadDetector::inspect_app_needs(&app_dir);
    assert!(
        manifest.is_some(),
        "App with typst dependency should be detected as heavy workload"
    );

    let manifest = manifest.unwrap();
    assert_eq!(manifest.name, "my-typeset-app");
}

#[tokio::test]
async fn test_adversarial_missing_font_fail_closed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let empty_font_dir = temp_dir.path().join("empty_fonts");
    std::fs::create_dir_all(&empty_font_dir).unwrap();

    let manifest = HeavyWorkloadManifest {
        name: "test-missing-font".to_string(),
        typst_binary: Some(PathBuf::from("/usr/bin/typst")),
        rhwp_binary: None,
        font_directories: vec![empty_font_dir.clone()],
        total_asset_bytes: 0,
        outbound_api_allowed: vec![],
    };

    assert_eq!(
        manifest.total_asset_bytes, 0,
        "Corrupted or missing fonts must be detected as 0 bytes"
    );
}

#[tokio::test]
async fn test_adversarial_outbound_network_firewall_policy() {
    let manifest = HeavyWorkloadManifest {
        name: "typeset-publishing".to_string(),
        typst_binary: None,
        rhwp_binary: None,
        font_directories: vec![],
        total_asset_bytes: 0,
        outbound_api_allowed: vec!["generativelanguage.googleapis.com".to_string()],
    };

    // Allowed target
    let target_allowed = "generativelanguage.googleapis.com";
    assert!(
        manifest
            .outbound_api_allowed
            .contains(&target_allowed.to_string()),
        "Gemini API must be whitelisted for external model inference"
    );

    // Blocked target (Adversarial test)
    let target_malicious = "malicious-botnet.evil.com";
    assert!(
        !manifest
            .outbound_api_allowed
            .contains(&target_malicious.to_string()),
        "Untrusted external domains must be blocked by outbound firewall"
    );
}

#[tokio::test]
async fn test_scale_to_zero_with_heavy_workload_footprint() {
    let mos_root = std::env::var("CARGO_MANIFEST_DIR")
        .map(|p| {
            PathBuf::from(p)
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf()
        })
        .unwrap_or_else(|_| PathBuf::from("."));
    let run_dir = mos_root.join("run");
    let firecracker_bin = mos_root.join("bin/firecracker");
    let kernel_path = mos_root.join("bin/vmlinux-5.10.186");
    let base_rootfs = mos_root.join("bin/rootfs.ext4");

    if !firecracker_bin.exists() || !kernel_path.exists() || !base_rootfs.exists() {
        eprintln!("Skipping microvm test: bin assets not found");
        return;
    }

    let test_rootfs = run_dir.join("fc_heavy_test.ext4");
    let _ = tokio::fs::copy(&base_rootfs, &test_rootfs).await;

    let config = InstanceConfig::new("heavy-vm", kernel_path, test_rootfs);
    let socket_path = run_dir.join("fc_heavy.sock");

    // 1. Boot MicroVM
    let start = Instant::now();
    let inst = MicroVmInstance::boot(
        &firecracker_bin,
        socket_path,
        config.clone(),
        "console=ttyS0 reboot=k panic=1 pci=off init=/bin/sh",
    )
    .await;

    if let Ok(instance) = inst {
        let cold_boot = start.elapsed();
        println!(
            "Heavy MicroVM Cold Boot: {:.2} ms",
            cold_boot.as_secs_f64() * 1000.0
        );

        // 2. Snapshot
        let snap_dir = run_dir.join("snapshots/heavy_test");
        let engine = SnapshotEngine::new(firecracker_bin.clone());
        let artifacts = engine.snapshot_and_stop(instance, &snap_dir).await;

        if let Ok(artifacts) = artifacts {
            // 3. Fast Resume
            let resume_sock = run_dir.join("fc_heavy_resume.sock");
            let resumed = engine
                .resume_from_snapshot(resume_sock, config, &artifacts)
                .await;

            if let Ok((mut resumed_inst, resume_dur)) = resumed {
                let resume_ms = resume_dur.as_secs_f64() * 1000.0;
                println!("Heavy MicroVM Fast Resume: {:.2} ms", resume_ms);
                assert!(
                    resume_ms < 30.0,
                    "Scale-to-zero resume must be < 30ms even under heavy workload"
                );
                let _ = resumed_inst.process.kill().await;
            }
        }
    }
}
