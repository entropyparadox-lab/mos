use m_os_orchestrator::{CgroupController, RateLimiterConfig, VsockDeviceConfig, VsockHostChannel};
use std::path::PathBuf;

#[test]
fn test_cgroup_controller_setup_and_teardown() {
    let mut cg = CgroupController::new("test-inst-123", Some(&PathBuf::from("/tmp/mos-cgroups")));
    let setup_res = cg.setup(12345, 2, 256);
    assert!(setup_res.is_ok());

    let destroy_res = cg.destroy();
    assert!(destroy_res.is_ok());
}

#[test]
fn test_rate_limiter_configs() {
    let net_rl = RateLimiterConfig::network_default();
    assert!(net_rl.bandwidth.is_some());
    assert_eq!(net_rl.bandwidth.as_ref().unwrap().size, 12_500_000);
    assert!(net_rl.ops.is_some());

    let disk_rl = RateLimiterConfig::disk_default();
    assert!(disk_rl.bandwidth.is_some());
    assert_eq!(disk_rl.bandwidth.as_ref().unwrap().size, 50_000_000);
    assert_eq!(disk_rl.ops.as_ref().unwrap().size, 2_000);
}

#[test]
fn test_vsock_device_config() {
    let vsock = VsockDeviceConfig::new(3, "/tmp/v.sock");
    assert_eq!(vsock.guest_cid, 3);
    assert_eq!(vsock.vsock_id, "vsock0");
    assert_eq!(vsock.uds_path, PathBuf::from("/tmp/v.sock"));

    let channel = VsockHostChannel::new(PathBuf::from("/tmp/nonexistent.sock"));
    assert_eq!(channel.socket_path, PathBuf::from("/tmp/nonexistent.sock"));
}
