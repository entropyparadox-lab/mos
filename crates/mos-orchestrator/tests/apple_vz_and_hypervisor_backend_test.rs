use m_os_orchestrator::backend::{AppleVzBackend, LinuxFirecrackerBackend, VzReactor};
use mos_core::backend::{BackendError, Feature, HypervisorBackend, MachineSpec, NetworkSpec};
use std::path::PathBuf;

#[tokio::test]
async fn test_vz_reactor_lifecycle_and_serial_isolation() {
    let reactor = VzReactor::spawn();

    let spec = MachineSpec::new(
        "mac-app-01",
        PathBuf::from("/runtime/kernels/vmlinux"),
        PathBuf::from("/runtime/rootfs/app.raw"),
    )
    .with_network(NetworkSpec::nat())
    .with_vcpu(2)
    .with_memory(256);

    let id = spec.id;

    // 1. Create
    let create_res = reactor.send_create(spec).await;
    assert!(create_res.is_ok());
    assert_eq!(create_res.unwrap(), id);

    // 2. Start
    let start_res = reactor.send_start(id).await;
    assert!(start_res.is_ok());

    // 3. Pause & Resume
    assert!(reactor.send_pause(id).await.is_ok());
    assert!(reactor.send_resume(id).await.is_ok());

    // 4. Shutdown & Dispose
    assert!(reactor.send_shutdown(id).await.is_ok());
    assert!(reactor.send_dispose(id).await.is_ok());

    // 5. Not found after dispose
    let restart_res = reactor.send_start(id).await;
    assert!(restart_res.is_err());
}

#[tokio::test]
async fn test_apple_vz_backend_trait_and_snapshot_gating() {
    let vz_backend = AppleVzBackend::new();

    // 1. Feature 검증 (NAT, Rosetta, VirtioFs, Vsock 지원, Snapshot 미지원)
    assert!(vz_backend.supports(Feature::NatNetwork));
    assert!(vz_backend.supports(Feature::Rosetta));
    assert!(vz_backend.supports(Feature::VirtioFs));
    assert!(!vz_backend.supports(Feature::SnapshotRestore));
    assert!(!vz_backend.supports(Feature::TapNetwork));

    // 2. 정상 생성
    let mut spec = MachineSpec::new(
        "vz-web-app",
        PathBuf::from("/kernel"),
        PathBuf::from("/rootfs"),
    )
    .with_network(NetworkSpec::nat());
    spec.enable_rosetta = true;

    let id = vz_backend.create(spec).await.expect("VZ create failed");

    // 3. 실행 및 일시정지
    assert!(vz_backend.start(id).await.is_ok());
    assert!(vz_backend.pause(id).await.is_ok());
    assert!(vz_backend.resume(id).await.is_ok());

    // 4. 스냅샷 시도 시 UnsupportedFeature 에러 반환 검증 (Apple private entitlement 제약 방어)
    let snap_res = vz_backend
        .snapshot(id, PathBuf::from("/tmp/test.snap"))
        .await;
    assert!(snap_res.is_err());
    match snap_res.unwrap_err() {
        BackendError::UnsupportedFeature(f) => assert_eq!(f, Feature::Snapshot),
        other => panic!("Expected UnsupportedFeature(Snapshot), got {:?}", other),
    }

    // 5. 정리
    assert!(vz_backend.shutdown(id).await.is_ok());
    assert!(vz_backend.dispose(id).await.is_ok());
}

#[tokio::test]
async fn test_linux_firecracker_backend_feature_set() {
    let fc_backend = LinuxFirecrackerBackend::new(
        PathBuf::from("/usr/bin/firecracker"),
        PathBuf::from("/tmp/mos-runtime"),
    );

    assert!(fc_backend.supports(Feature::Snapshot));
    assert!(fc_backend.supports(Feature::SnapshotRestore));
    assert!(fc_backend.supports(Feature::UffdLazyRestore));
    assert!(fc_backend.supports(Feature::TapNetwork));
    assert!(!fc_backend.supports(Feature::NatNetwork));
    assert!(!fc_backend.supports(Feature::Rosetta));
}
