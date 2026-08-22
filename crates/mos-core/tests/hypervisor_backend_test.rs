use m_os_core::backend::{
    BackendError, Feature, HypervisorBackend, MachineSpec, MockHypervisorBackend, NetworkSpec,
};
use std::path::PathBuf;

#[tokio::test]
async fn test_machine_spec_builder_and_validation() {
    let spec = MachineSpec::new(
        "test-app",
        PathBuf::from("/runtime/kernels/vmlinux"),
        PathBuf::from("/runtime/rootfs/app.ext4"),
    )
    .with_vcpu(2)
    .with_memory(256)
    .with_network(NetworkSpec::tap("tap-mos-01", "172.16.0.2"))
    .with_vsock(10700);

    assert_eq!(spec.name, "test-app");
    assert_eq!(spec.vcpu_count, 2);
    assert_eq!(spec.mem_size_mib, 256);
    assert_eq!(spec.networks.len(), 1);
    assert_eq!(spec.networks[0].interface_name, "eth0");
    assert_eq!(spec.vsock_port, Some(10700));
}

#[tokio::test]
async fn test_hypervisor_backend_supported_features_and_gating() {
    // Linux KVM 스타일 Mock 백엔드 (Snapshot, Tap, Vsock 지원, Rosetta/NAT 미지원)
    let linux_backend = MockHypervisorBackend::new([
        Feature::Snapshot,
        Feature::SnapshotRestore,
        Feature::UffdLazyRestore,
        Feature::TapNetwork,
        Feature::Vsock,
        Feature::Adoption,
    ]);

    assert!(linux_backend.supports(Feature::Snapshot));
    assert!(linux_backend.supports(Feature::TapNetwork));
    assert!(!linux_backend.supports(Feature::NatNetwork));
    assert!(!linux_backend.supports(Feature::Rosetta));

    // Tap 네트워크를 요구하는 정상 스펙
    let valid_spec = MachineSpec::new(
        "linux-vm",
        PathBuf::from("/kernel"),
        PathBuf::from("/rootfs"),
    )
    .with_network(NetworkSpec::tap("tap0", "172.16.0.2"));

    let create_res = linux_backend.create(valid_spec).await;
    assert!(create_res.is_ok());

    // NAT 네트워크를 요구하는 잘못된 스펙 (Linux 백엔드에서 거부되어야 함)
    let invalid_nat_spec = MachineSpec::new(
        "invalid-vm",
        PathBuf::from("/kernel"),
        PathBuf::from("/rootfs"),
    )
    .with_network(NetworkSpec::nat());

    let nat_res = linux_backend.create(invalid_nat_spec).await;
    assert!(nat_res.is_err());
    match nat_res.unwrap_err() {
        BackendError::UnsupportedFeature(f) => assert_eq!(f, Feature::NatNetwork),
        other => panic!("Expected UnsupportedFeature(NatNetwork), got {:?}", other),
    }

    // Rosetta를 요구하는 잘못된 스펙
    let mut rosetta_spec = MachineSpec::new(
        "rosetta-vm",
        PathBuf::from("/kernel"),
        PathBuf::from("/rootfs"),
    );
    rosetta_spec.enable_rosetta = true;

    let rosetta_res = linux_backend.create(rosetta_spec).await;
    assert!(rosetta_res.is_err());
    match rosetta_res.unwrap_err() {
        BackendError::UnsupportedFeature(f) => assert_eq!(f, Feature::Rosetta),
        other => panic!("Expected UnsupportedFeature(Rosetta), got {:?}", other),
    }
}

#[tokio::test]
async fn test_macos_vz_style_backend_gating() {
    // macOS Apple VZ 스타일 Mock 백엔드 (NAT, Rosetta, Vsock 지원, Snapshot/Tap 미지원)
    let macos_backend = MockHypervisorBackend::new([
        Feature::NatNetwork,
        Feature::VirtioFs,
        Feature::Rosetta,
        Feature::Vsock,
    ]);

    assert!(macos_backend.supports(Feature::NatNetwork));
    assert!(macos_backend.supports(Feature::Rosetta));
    assert!(!macos_backend.supports(Feature::SnapshotRestore));
    assert!(!macos_backend.supports(Feature::TapNetwork));

    // macOS용 정상 스펙 (NAT + Rosetta)
    let mut mac_spec =
        MachineSpec::new("mac-vm", PathBuf::from("/kernel"), PathBuf::from("/rootfs"))
            .with_network(NetworkSpec::nat());
    mac_spec.enable_rosetta = true;

    assert!(macos_backend.create(mac_spec).await.is_ok());

    // macOS에서 스냅샷 복구 시도 시 에러 검증
    let snapshot_spec = MachineSpec::new(
        "snap-vm",
        PathBuf::from("/kernel"),
        PathBuf::from("/rootfs"),
    )
    .with_network(NetworkSpec::nat())
    .with_snapshot_restore(PathBuf::from("/snapshots/vm.snap"));

    let snap_res = macos_backend.create(snapshot_spec).await;
    assert!(snap_res.is_err());
    match snap_res.unwrap_err() {
        BackendError::UnsupportedFeature(f) => assert_eq!(f, Feature::SnapshotRestore),
        other => panic!(
            "Expected UnsupportedFeature(SnapshotRestore), got {:?}",
            other
        ),
    }
}
