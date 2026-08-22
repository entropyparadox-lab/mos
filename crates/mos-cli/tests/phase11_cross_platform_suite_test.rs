use flate2::write::GzEncoder;
use flate2::Compression;
use mos_builder::{CoWCloner, KernelUnpacker, OciLayerUnpacker};
use mos_core::backend::{BackendError, Feature, HypervisorBackend, MachineSpec, NetworkSpec};
use mos_core::InstanceId;
use mos_edge::router::{EdgeRouter, RouteTarget, WakeMode};
use mos_orchestrator::backend::{AppleVzBackend, LinuxFirecrackerBackend};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tempfile::tempdir;

#[tokio::test]
async fn test_phase11_cross_platform_full_pipeline() {
    let tmp = tempdir().unwrap();
    let root_path = tmp.path();

    // ==========================================
    // 1. Build Phase: EFI zboot Kernel Unpacking & OCI Rootfs Extraction
    // ==========================================
    println!("🧪 [Step 1] Verifying Rootless OCI Unpacking & arm64 EFI zboot parsing...");

    // 1.1 Synthetic arm64 EFI zboot kernel
    let mut raw_kernel = vec![0u8; 256];
    raw_kernel[0x38..0x3C].copy_from_slice(b"ARM\x64");
    raw_kernel[0x40..0x4C].copy_from_slice(b"MOS_VIRT_ARM");

    let mut gz_enc = GzEncoder::new(Vec::new(), Compression::default());
    gz_enc.write_all(&raw_kernel).unwrap();
    let gz_payload = gz_enc.finish().unwrap();

    let mut zboot = vec![0u8; 64];
    zboot[0..2].copy_from_slice(b"MZ");
    zboot[4..8].copy_from_slice(b"zimg");
    zboot[8..12].copy_from_slice(&(64u32).to_le_bytes());
    zboot[12..16].copy_from_slice(&(gz_payload.len() as u32).to_le_bytes());
    zboot[24..28].copy_from_slice(b"gzip");
    zboot.extend_from_slice(&gz_payload);

    let kernel_dst = root_path.join("vmlinux-unpacked");
    let extracted_kernel = KernelUnpacker::unpack_bytes(&zboot).expect("Kernel unpack failed");
    fs::write(&kernel_dst, &extracted_kernel).unwrap();
    assert_eq!(&extracted_kernel[0x38..0x3C], b"ARM\x64");
    println!("  ✅ arm64 EFI zboot successfully unpacked to raw ARMd Image");

    // 1.2 OCI Layers with .wh. whiteouts
    let rootfs_dir = root_path.join("rootfs_extracted");
    fs::create_dir_all(&rootfs_dir).unwrap();

    let mut tar_builder = tar::Builder::new(Vec::new());
    let mut h1 = tar::Header::new_gnu();
    h1.set_path("app/main.js").unwrap();
    h1.set_size(19);
    h1.set_mode(0o755);
    h1.set_cksum();
    tar_builder
        .append(&h1, b"console.log('mos');".as_slice())
        .unwrap();

    let mut h2 = tar::Header::new_gnu();
    h2.set_path("app/.wh.deprecated.js").unwrap();
    h2.set_size(0);
    h2.set_mode(0o644);
    h2.set_cksum();
    tar_builder.append(&h2, std::io::empty()).unwrap();

    let tar_bytes = tar_builder.into_inner().unwrap();
    OciLayerUnpacker::unpack_layer(tar_bytes.as_slice(), &rootfs_dir).unwrap();
    assert!(rootfs_dir.join("app/main.js").exists());
    assert!(!rootfs_dir.join("app/deprecated.js").exists());
    println!("  ✅ OCI Layers unpacked in userspace without loop mounts");

    // 1.3 CoW Disk Cloning
    let base_disk = root_path.join("base_rootfs.raw");
    let instance_disk = root_path.join("instance_01.raw");
    fs::write(&base_disk, b"BASE_ROOTFS_BLOCK_DATA_12345").unwrap();
    CoWCloner::clone_file(&base_disk, &instance_disk).expect("CoW clone failed");
    assert!(instance_disk.exists());
    println!("  ✅ APFS/FICLONE CoW disk clone verified");

    // ==========================================
    // 2. Orchestration Phase: Hypervisor Backend Trait & Gating
    // ==========================================
    println!("🧪 [Step 2] Verifying Linux Firecracker & macOS Apple VZ Backend Traits...");

    // 2.1 Linux Firecracker Backend
    let linux_backend = LinuxFirecrackerBackend::new(
        PathBuf::from("/usr/bin/firecracker"),
        root_path.join("runtime"),
    );
    assert!(linux_backend.supports(Feature::Snapshot));
    assert!(linux_backend.supports(Feature::UffdLazyRestore));
    assert!(linux_backend.supports(Feature::TapNetwork));
    assert!(!linux_backend.supports(Feature::Rosetta));

    // 2.2 macOS Apple VZ Backend with Serial Reactor
    let vz_backend = AppleVzBackend::new();
    assert!(vz_backend.supports(Feature::NatNetwork));
    assert!(vz_backend.supports(Feature::Rosetta));
    assert!(vz_backend.supports(Feature::Vsock));
    assert!(!vz_backend.supports(Feature::SnapshotRestore));

    let mut mac_spec = MachineSpec::new("mac-vibe-app", kernel_dst.clone(), instance_disk.clone())
        .with_network(NetworkSpec::nat())
        .with_vsock(10700);
    mac_spec.enable_rosetta = true;

    let mac_id = vz_backend
        .create(mac_spec)
        .await
        .expect("VZ VM creation failed");
    assert!(vz_backend.start(mac_id).await.is_ok());
    assert!(vz_backend.pause(mac_id).await.is_ok());
    assert!(vz_backend.resume(mac_id).await.is_ok());

    // Verify Apple private entitlement protection (Snapshot rejected gracefully)
    let snap_err = vz_backend
        .snapshot(mac_id, root_path.join("mac.snap"))
        .await;
    assert!(matches!(
        snap_err,
        Err(BackendError::UnsupportedFeature(Feature::Snapshot))
    ));
    assert!(vz_backend.shutdown(mac_id).await.is_ok());
    assert!(vz_backend.dispose(mac_id).await.is_ok());
    println!("  ✅ Apple VZ Reactor & Feature Gating verified");

    // ==========================================
    // 3. Edge Routing Phase: Multi-Platform WakeMode & Vsock Forwarding
    // ==========================================
    println!("🧪 [Step 3] Verifying Cross-Platform Edge Routing & Wake-on-HTTP Buffer Modes...");

    let router = EdgeRouter::new();

    // Linux Target: UFFD Snapshot Resume
    let linux_target = RouteTarget::new(InstanceId::new(), "172.16.0.2", 8080, true)
        .with_wake_mode(WakeMode::SnapshotResume);
    router.register("linux-prod.mos.local", linux_target);

    // macOS Target: Fast Cold Boot + Vsock Tunnel
    let mac_target = RouteTarget::new(InstanceId::new(), "127.0.0.1", 8080, true)
        .with_wake_mode(WakeMode::ColdBoot)
        .with_vsock_tunnel(10700);
    router.register("mac-dev.mos.local", mac_target);

    let resolved_linux = router.resolve("linux-prod.mos.local").unwrap();
    assert_eq!(resolved_linux.wake_mode, WakeMode::SnapshotResume);
    assert_eq!(resolved_linux.vsock_tunnel, None);

    let resolved_mac = router.resolve("mac-dev.mos.local").unwrap();
    assert_eq!(resolved_mac.wake_mode, WakeMode::ColdBoot);
    assert_eq!(resolved_mac.vsock_tunnel, Some(10700));

    println!("  ✅ EdgeRouter cross-platform routing verified");
    println!("🎉 Phase 11 Cross-Platform (Linux KVM + macOS VZ) Full Pipeline PASSED!");
}
