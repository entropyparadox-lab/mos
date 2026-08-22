use m_os::HostProvisioner;
use mos_core::{
    Ed25519AuthManager, InstanceId, RbacTokenPayload, Role, TenantId, TenantManager,
    TenantNamespace,
};
use mos_orchestrator::{GpuDevice, GpuPoolManager};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn test_phase7_production_ga_full_pipeline() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let base_dir = temp_dir.path().join("mos_root");

    // 1. Baremetal Provisioning & Systemd Generation
    let provisioner = HostProvisioner::new(&base_dir);
    let preflight = provisioner.run_preflight();
    assert!(preflight.storage_writable);

    provisioner
        .provision_directories()
        .expect("Failed provisioning directories");
    let unit = provisioner
        .generate_systemd_unit(&base_dir.join("bin/mos"), &base_dir.join("config/mos.toml"));
    assert!(unit.contains("Description=MOS (MicroVM Operating Service) Node Daemon"));

    // 2. Multi-Tenant Namespace Quota & Ed25519 RBAC Auth
    let tenant_mgr = TenantManager::new();
    let tenant_id = TenantId("enterprise-tenant-alpha".to_string());
    tenant_mgr.register_tenant(TenantNamespace::new(
        "enterprise-tenant-alpha",
        "Alpha Enterprise",
        5,
        1024,
        8,
    ));

    // Allocate 512MB RAM & 4 vCPUs for Tenant Alpha
    tenant_mgr
        .allocate(&tenant_id, 512, 4)
        .expect("Tenant allocation should succeed");
    let tenant = tenant_mgr.get_tenant(&tenant_id).unwrap();
    assert_eq!(tenant.allocated_ram_mib, 512);
    assert_eq!(tenant.allocated_vcpu, 4);

    let auth_mgr = Ed25519AuthManager::new_random();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let token_payload = RbacTokenPayload {
        token_id: "token-prod-ga-01".to_string(),
        tenant_id: "enterprise-tenant-alpha".to_string(),
        role: Role::Admin,
        issued_at: now,
        expires_at: now + 7200,
    };
    let token_str = auth_mgr
        .sign_token(&token_payload)
        .expect("Token sign failed");
    let verified_payload = auth_mgr
        .verify_token(&token_str)
        .expect("Token verify failed");
    assert_eq!(verified_payload.role, Role::Admin);

    // 3. GPU Scale-to-Zero Dynamic VRAM Allocation & Detach
    let gpu_pool = GpuPoolManager::new();
    let gpu_h100 = GpuDevice::new(0, "NVIDIA H100 80GB SXM5", "0000:07:00.0", 81920);
    gpu_pool.register_device(gpu_h100);

    let inst_llm = InstanceId::new();
    let binding = gpu_pool
        .bind_gpu_to_instance(&inst_llm, 40960) // 40GB for DeepSeek-R1-Q4
        .expect("GPU binding failed");

    assert_eq!(binding.gpu_index, 0);
    assert_eq!(gpu_pool.total_vram_in_use_mib(), 40960);

    // Scale-to-Zero trigger on idle -> 0MB VRAM
    let freed = gpu_pool
        .scale_to_zero_detach(&inst_llm)
        .expect("GPU detach failed");
    assert_eq!(freed, 40960);
    assert_eq!(gpu_pool.total_vram_in_use_mib(), 0);
}
