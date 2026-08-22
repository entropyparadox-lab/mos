use m_os_edge::router::{EdgeRouter, RouteTarget, WakeMode};
use mos_core::InstanceId;

#[tokio::test]
async fn test_edge_router_wake_mode_and_vsock_tunnel_config() {
    let router = EdgeRouter::new();

    // 1. Linux SnapshotResume 타깃 등록
    let linux_target = RouteTarget {
        instance_id: InstanceId::new(),
        host: "172.16.0.2".into(),
        port: 8080,
        is_suspended: true,
        wake_mode: WakeMode::SnapshotResume,
        vsock_tunnel: None,
    };
    router.register("linux-app.mos.local", linux_target);

    let resolved_linux = router.resolve("linux-app.mos.local").unwrap();
    assert_eq!(resolved_linux.wake_mode, WakeMode::SnapshotResume);
    assert!(resolved_linux.is_suspended);
    assert_eq!(resolved_linux.vsock_tunnel, None);

    // 2. macOS ColdBoot + Vsock 타깃 등록
    let macos_target = RouteTarget {
        instance_id: InstanceId::new(),
        host: "127.0.0.1".into(),
        port: 8080,
        is_suspended: true,
        wake_mode: WakeMode::ColdBoot,
        vsock_tunnel: Some(10700),
    };
    router.register("macos-app.mos.local", macos_target);

    let resolved_mac = router.resolve("macos-app.mos.local").unwrap();
    assert_eq!(resolved_mac.wake_mode, WakeMode::ColdBoot);
    assert_eq!(resolved_mac.vsock_tunnel, Some(10700));
}
