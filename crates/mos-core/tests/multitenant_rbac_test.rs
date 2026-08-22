use m_os_core::{
    Ed25519AuthManager, QuotaError, RbacTokenPayload, Role, TenantId, TenantManager,
    TenantNamespace,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn test_tenant_resource_quota_enforcement() {
    let mgr = TenantManager::new();
    let tenant_id = TenantId("tenant-vibe-corp".to_string());

    // Quota: Max 2 VMs, Max 256MB RAM, Max 2 vCPUs
    let namespace = TenantNamespace::new("tenant-vibe-corp", "Vibe Corp", 2, 256, 2);
    mgr.register_tenant(namespace);

    // 1. Allocate VM 1 (128MB RAM, 1 vCPU) -> OK
    mgr.allocate(&tenant_id, 128, 1)
        .expect("Allocation 1 should succeed");
    let t = mgr.get_tenant(&tenant_id).unwrap();
    assert_eq!(t.active_vms, 1);
    assert_eq!(t.allocated_ram_mib, 128);

    // 2. Allocate VM 2 (128MB RAM, 1 vCPU) -> OK
    mgr.allocate(&tenant_id, 128, 1)
        .expect("Allocation 2 should succeed");
    let t = mgr.get_tenant(&tenant_id).unwrap();
    assert_eq!(t.active_vms, 2);
    assert_eq!(t.allocated_ram_mib, 256);

    // 3. Allocate VM 3 -> Fails on Max VMs quota
    let err = mgr.allocate(&tenant_id, 64, 1).unwrap_err();
    assert_eq!(
        err,
        QuotaError::ExceededMaxVms {
            active: 2,
            limit: 2
        }
    );

    // 4. Release VM 1 and reallocate
    mgr.release(&tenant_id, 128, 1);
    let t = mgr.get_tenant(&tenant_id).unwrap();
    assert_eq!(t.active_vms, 1);
    assert_eq!(t.allocated_ram_mib, 128);

    // Allocate oversized RAM (200MB > 128MB avail) -> Fails on RAM quota
    let err_ram = mgr.allocate(&tenant_id, 200, 1).unwrap_err();
    assert_eq!(
        err_ram,
        QuotaError::ExceededMaxRam {
            requested_mib: 200,
            available_mib: 128
        }
    );
}

#[test]
fn test_ed25519_rbac_token_sign_and_verify() {
    let auth = Ed25519AuthManager::new_random();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let payload = RbacTokenPayload {
        token_id: "tok-admin-01".to_string(),
        tenant_id: "tenant-vibe-corp".to_string(),
        role: Role::Admin,
        issued_at: now,
        expires_at: now + 3600, // Valid 1 hour
    };

    // 1. Sign token
    let token_str = auth.sign_token(&payload).expect("Token signing failed");
    assert!(token_str.contains('.'));

    // 2. Verify token
    let verified = auth.verify_token(&token_str).expect("Verification failed");
    assert_eq!(verified, payload);
    assert_eq!(verified.role, Role::Admin);

    // 3. Tampered token verification fails
    let tampered = format!("tampered_{}", token_str);
    assert!(auth.verify_token(&tampered).is_err());

    // 4. Expired token verification fails
    let expired_payload = RbacTokenPayload {
        token_id: "tok-exp-01".to_string(),
        tenant_id: "tenant-vibe-corp".to_string(),
        role: Role::Viewer,
        issued_at: now - 3600,
        expires_at: now - 10,
    };
    let expired_token_str = auth.sign_token(&expired_payload).unwrap();
    let expired_res = auth.verify_token(&expired_token_str);
    assert!(expired_res.is_err());
}
