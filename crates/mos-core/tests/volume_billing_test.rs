use m_os_core::{
    BillingEngine, BillingRate, CreditAccount, UsageMetric, VolumeAccessMode, VolumeConfig,
    VolumeQuota,
};
use std::path::PathBuf;

#[test]
fn test_volume_config_and_access_modes() {
    let vol = VolumeConfig::new(
        "shared-data",
        "tenant-corp-1",
        10 * 1024 * 1024 * 1024,
        PathBuf::from("/var/lib/mos/volumes/shared-data"),
        "/data",
        VolumeAccessMode::ReadWriteMany,
    );

    assert_eq!(vol.name, "shared-data");
    assert_eq!(vol.tenant_id, "tenant-corp-1");
    assert_eq!(vol.access_mode, VolumeAccessMode::ReadWriteMany);
    assert_eq!(vol.capacity_bytes, 10 * 1024 * 1024 * 1024);

    let quota = VolumeQuota::default();
    assert_eq!(quota.max_volumes, 10);
}

#[test]
fn test_billing_engine_usage_and_credit_deduction() {
    let rate = BillingRate::default();
    let engine = BillingEngine::new(rate);

    let account = CreditAccount::new("tenant-corp-1", 10.0); // 10 credits
    engine.register_account(account);

    // 1 hour run with 2 vCPU, 2 GiB RAM, 1 GiB VRAM, 100MB egress
    let metric = UsageMetric {
        vcpu_seconds: 7200.0,
        ram_gib_seconds: 7200.0,
        vram_gib_seconds: 3600.0,
        egress_bytes: 100 * 1024 * 1024,
    };

    let cost = engine.calculate_cost(&metric);
    assert!(cost > 0.0);
    assert!(cost < 1.0);

    let remaining = engine
        .charge_usage("tenant-corp-1", &metric)
        .expect("Charge should succeed");
    assert_eq!(remaining, 10.0 - cost);

    // Top-up
    let updated = engine.topup_credit("tenant-corp-1", 5.0).unwrap();
    assert_eq!(updated, (10.0 - cost) + 5.0);

    // Test suspension on heavy usage
    let heavy_metric = UsageMetric {
        vcpu_seconds: 100_000_000.0,
        ram_gib_seconds: 100_000_000.0,
        vram_gib_seconds: 100_000_000.0,
        egress_bytes: 0,
    };
    let err = engine
        .charge_usage("tenant-corp-1", &heavy_metric)
        .unwrap_err();
    assert!(format!("{}", err).contains("Account suspended"));

    let acc = engine.get_account("tenant-corp-1").unwrap();
    assert!(acc.is_suspended);
}
