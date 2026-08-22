use m_os_orchestrator::{UsageTracker, VolumeManager};
use mos_core::{
    BillingEngine, BillingRate, CreditAccount, InstanceId, VolumeAccessMode, VolumeQuota,
};
use std::thread::sleep;
use std::time::Duration;

#[test]
fn test_volume_manager_lifecycle_and_exclusive_locking() {
    let temp_dir = tempfile::tempdir().unwrap();
    let vol_mgr = VolumeManager::new(temp_dir.path());

    // 1. Set tenant quota
    let quota = VolumeQuota {
        max_volumes: 2,
        max_total_storage_bytes: 5 * 1024 * 1024 * 1024,
    };
    vol_mgr.set_tenant_quota("tenant-beta", quota);

    // 2. Create ReadWriteOnce volume
    let vol_rwo = vol_mgr
        .create_volume(
            "tenant-beta",
            "db-data",
            1024 * 1024 * 1024,
            "/var/lib/postgresql/data",
            VolumeAccessMode::ReadWriteOnce,
        )
        .expect("Volume creation should succeed");

    // 3. Attach to Instance 1
    let inst_1 = InstanceId::new();
    let attach_1 = vol_mgr
        .attach_volume(&vol_rwo.id, &inst_1, "tenant-beta")
        .expect("Attachment to inst_1 should succeed");
    assert_eq!(attach_1.mount_path, "/var/lib/postgresql/data");
    assert!(!attach_1.read_only);

    // 4. Attach to Instance 2 (Should fail because ReadWriteOnce is exclusive)
    let inst_2 = InstanceId::new();
    let attach_2_err = vol_mgr.attach_volume(&vol_rwo.id, &inst_2, "tenant-beta");
    assert!(attach_2_err.is_err());
    assert!(format!("{}", attach_2_err.unwrap_err()).contains("already mounted exclusively"));

    // 5. Access denied for another tenant
    let alien_attach_err = vol_mgr.attach_volume(&vol_rwo.id, &inst_2, "tenant-alien");
    assert!(alien_attach_err.is_err());
    assert!(format!("{}", alien_attach_err.unwrap_err()).contains("Access denied"));

    // 6. Detach inst 1 and then inst 2 can attach
    assert!(vol_mgr.detach_volume(&vol_rwo.id, &inst_1));
    let attach_2_ok = vol_mgr
        .attach_volume(&vol_rwo.id, &inst_2, "tenant-beta")
        .expect("Attachment to inst_2 should succeed after detach");
    assert_eq!(attach_2_ok.instance_id, inst_2);
}

#[test]
fn test_usage_tracker_and_billing_integration() {
    let billing = BillingEngine::new(BillingRate::default());
    billing.register_account(CreditAccount::new("tenant-gamma", 50.0));

    let tracker = UsageTracker::new(billing.clone());

    let inst_id = InstanceId::new();
    // Start tracking: 2 vCPU, 1024MB RAM, 4096MB VRAM
    tracker.start_tracking(inst_id, "tenant-gamma", 2, 1024, 4096);

    sleep(Duration::from_millis(50));
    tracker.record_egress(&inst_id, 1024 * 1024); // 1MB egress

    let charge_results = tracker.tick_and_charge();
    assert_eq!(charge_results.len(), 1);
    let (tenant, res) = &charge_results[0];
    assert_eq!(tenant, "tenant-gamma");
    assert!(res.is_ok());

    let new_balance = res.as_ref().unwrap();
    assert!(*new_balance < 50.0);
    assert!(*new_balance > 49.0);

    let metric = tracker.get_tenant_metric("tenant-gamma");
    assert!(metric.vcpu_seconds > 0.0);
    assert_eq!(metric.egress_bytes, 1024 * 1024);

    tracker.stop_tracking(&inst_id);
}
