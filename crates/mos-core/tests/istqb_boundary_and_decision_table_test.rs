use m_os_core::{
    BillingEngine, BillingRate, CreditAccount, Ed25519AuthManager, InstanceState, QuotaError,
    RbacTokenPayload, Role, TenantId, TenantManager, TenantNamespace, UsageMetric,
};
use std::time::{SystemTime, UNIX_EPOCH};

// =============================================================================
// ISTQB CTFL §4.2.1: Equivalence Partitioning (동등 분할) &
// ISTQB CTFL §4.2.2: Boundary Value Analysis (경계값 분석 - 2/3-Point BVA)
// =============================================================================

#[test]
fn test_istqb_bva_token_expiration_boundaries() {
    let auth = Ed25519AuthManager::new_random();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 1. Boundary 1: Just Expired (now - 1 sec) -> Must Reject
    let expired_payload = RbacTokenPayload {
        token_id: "tok-bva-01".to_string(),
        tenant_id: "tenant-bva".to_string(),
        role: Role::Developer,
        issued_at: now - 3600,
        expires_at: now - 1,
    };
    let tok_expired = auth.sign_token(&expired_payload).unwrap();
    let verify_expired = auth.verify_token(&tok_expired);
    assert!(
        verify_expired.is_err(),
        "Expired token at now-1s must be rejected"
    );

    // 2. Boundary 2: Valid at exact upper boundary (now + 1 sec) -> Must Accept
    let valid_payload = RbacTokenPayload {
        token_id: "tok-bva-02".to_string(),
        tenant_id: "tenant-bva".to_string(),
        role: Role::Developer,
        issued_at: now,
        expires_at: now + 2,
    };
    let tok_valid = auth.sign_token(&valid_payload).unwrap();
    let verify_valid = auth.verify_token(&tok_valid);
    assert!(
        verify_valid.is_ok(),
        "Valid token at now+2s must be accepted"
    );
}

#[test]
fn test_istqb_bva_tenant_resource_quota_limits() {
    let tenant_mgr = TenantManager::new();
    let tenant_id = TenantId("tenant-bva-quota".to_string());

    // Quota: Exactly 5 VMs, Exactly 1024 MiB RAM, Exactly 4 vCPUs
    tenant_mgr.register_tenant(TenantNamespace::new(
        "tenant-bva-quota",
        "BVA Quota Test",
        5,
        1024,
        4,
    ));

    // Boundary 1: Exact max quota allocation (1024 MiB, 4 vCPU) -> Must PASS
    let alloc_exact = tenant_mgr.allocate(&tenant_id, 1024, 4);
    assert!(alloc_exact.is_ok());

    // Boundary 2: 1 MiB over quota (1024 + 1 MiB) -> Must REJECT with QuotaExceeded
    let alloc_over_ram = tenant_mgr.allocate(&tenant_id, 1, 0);
    assert!(matches!(
        alloc_over_ram,
        Err(QuotaError::ExceededMaxRam { .. })
    ));

    // Boundary 3: 1 vCPU over quota (4 + 1 vCPU) -> Must REJECT with QuotaExceeded
    let alloc_over_cpu = tenant_mgr.allocate(&tenant_id, 0, 1);
    assert!(matches!(
        alloc_over_cpu,
        Err(QuotaError::ExceededMaxVcpu { .. })
    ));
}

#[test]
fn test_istqb_bva_credit_balance_zero_boundary() {
    let billing = BillingEngine::new(BillingRate::default());
    let account = CreditAccount::new("tenant-zero-boundary", 0.0001); // tiny positive
    billing.register_account(account);

    // Charge that leaves balance just slightly above 0
    let metric_tiny = UsageMetric {
        vcpu_seconds: 1.0, // 0.000010
        ram_gib_seconds: 0.0,
        vram_gib_seconds: 0.0,
        egress_bytes: 0,
    };
    let res = billing.charge_usage("tenant-zero-boundary", &metric_tiny);
    assert!(res.is_ok());
    let acc = billing.get_account("tenant-zero-boundary").unwrap();
    assert!(!acc.is_suspended);

    // Charge that depletes balance below or equal to 0.0 -> Must Trigger Suspension
    let metric_drain = UsageMetric {
        vcpu_seconds: 100.0, // 0.001000 cost > remaining balance
        ram_gib_seconds: 0.0,
        vram_gib_seconds: 0.0,
        egress_bytes: 0,
    };
    let res_drain = billing.charge_usage("tenant-zero-boundary", &metric_drain);
    assert!(
        res_drain.is_err(),
        "Depleted balance must return suspension error"
    );
    let acc_drain = billing.get_account("tenant-zero-boundary").unwrap();
    assert!(acc_drain.is_suspended);
}

// =============================================================================
// ISTQB CTFL §4.2.4: Decision Table Testing (의사결정 테이블 테스팅)
//
// Rule Matrix for Action Authorization:
// | Rule | Role      | Token Valid? | Tenant Match? | Action: Read | Action: Deploy | Action: Delete |
// | :--- | :-------- | :----------- | :------------ | :----------- | :------------- | :------------- |
// | R1   | Viewer    | Yes          | Yes           | ALLOW (T)    | DENY (F)       | DENY (F)       |
// | R2   | Developer | Yes          | Yes           | ALLOW (T)    | ALLOW (T)      | DENY (F)       |
// | R3   | Admin     | Yes          | Yes           | ALLOW (T)    | ALLOW (T)      | ALLOW (T)      |
// | R4   | Admin     | Expired      | Yes           | DENY (F)     | DENY (F)       | DENY (F)       |
// | R5   | Admin     | Yes          | No (Cross)    | DENY (F)     | DENY (F)       | DENY (F)       |
// =============================================================================

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Read,
    Deploy,
    Delete,
}

fn check_permission(
    role: Role,
    token_valid: bool,
    token_tenant: &str,
    target_tenant: &str,
    action: Action,
) -> bool {
    if !token_valid || token_tenant != target_tenant {
        return false;
    }
    match (role, action) {
        (Role::Viewer, Action::Read) => true,
        (Role::Viewer, _) => false,
        (Role::Developer, Action::Read | Action::Deploy) => true,
        (Role::Developer, Action::Delete) => false,
        (Role::Admin, _) => true,
    }
}

#[test]
fn test_istqb_decision_table_rbac_actions() {
    // Rule 1: Viewer
    assert!(check_permission(
        Role::Viewer,
        true,
        "org-a",
        "org-a",
        Action::Read
    ));
    assert!(!check_permission(
        Role::Viewer,
        true,
        "org-a",
        "org-a",
        Action::Deploy
    ));
    assert!(!check_permission(
        Role::Viewer,
        true,
        "org-a",
        "org-a",
        Action::Delete
    ));

    // Rule 2: Developer
    assert!(check_permission(
        Role::Developer,
        true,
        "org-a",
        "org-a",
        Action::Read
    ));
    assert!(check_permission(
        Role::Developer,
        true,
        "org-a",
        "org-a",
        Action::Deploy
    ));
    assert!(!check_permission(
        Role::Developer,
        true,
        "org-a",
        "org-a",
        Action::Delete
    ));

    // Rule 3: Admin
    assert!(check_permission(
        Role::Admin,
        true,
        "org-a",
        "org-a",
        Action::Read
    ));
    assert!(check_permission(
        Role::Admin,
        true,
        "org-a",
        "org-a",
        Action::Deploy
    ));
    assert!(check_permission(
        Role::Admin,
        true,
        "org-a",
        "org-a",
        Action::Delete
    ));

    // Rule 4: Expired Token
    assert!(!check_permission(
        Role::Admin,
        false,
        "org-a",
        "org-a",
        Action::Read
    ));
    assert!(!check_permission(
        Role::Admin,
        false,
        "org-a",
        "org-a",
        Action::Deploy
    ));
    assert!(!check_permission(
        Role::Admin,
        false,
        "org-a",
        "org-a",
        Action::Delete
    ));

    // Rule 5: Cross-Tenant Access
    assert!(!check_permission(
        Role::Admin,
        true,
        "org-a",
        "org-b",
        Action::Read
    ));
    assert!(!check_permission(
        Role::Admin,
        true,
        "org-a",
        "org-b",
        Action::Deploy
    ));
    assert!(!check_permission(
        Role::Admin,
        true,
        "org-a",
        "org-b",
        Action::Delete
    ));
}

// =============================================================================
// ISTQB CTFL §4.2.3: State Transition Testing (상태 전이 테스팅)
// Valid Lifecycle: Building -> Starting -> Running -> Suspended -> Running -> Stopped
// Invalid: Suspended -> Building, Stopped -> Suspended
// =============================================================================

fn can_transition(from: InstanceState, to: InstanceState) -> bool {
    matches!(
        (from, to),
        (InstanceState::Building, InstanceState::Starting)
            | (InstanceState::Building, InstanceState::Failed)
            | (InstanceState::Starting, InstanceState::Running)
            | (InstanceState::Starting, InstanceState::Failed)
            | (InstanceState::Running, InstanceState::Suspended)
            | (InstanceState::Running, InstanceState::Stopped)
            | (InstanceState::Running, InstanceState::Failed)
            | (InstanceState::Suspended, InstanceState::Running)
            | (InstanceState::Suspended, InstanceState::Stopped)
            | (InstanceState::Stopped, InstanceState::Starting)
            | (InstanceState::Failed, InstanceState::Building)
    )
}

#[test]
fn test_istqb_state_transition_matrix() {
    // Valid transitions
    assert!(can_transition(
        InstanceState::Building,
        InstanceState::Starting
    ));
    assert!(can_transition(
        InstanceState::Starting,
        InstanceState::Running
    ));
    assert!(can_transition(
        InstanceState::Running,
        InstanceState::Suspended
    ));
    assert!(can_transition(
        InstanceState::Suspended,
        InstanceState::Running
    )); // Wake-on-HTTP
    assert!(can_transition(
        InstanceState::Running,
        InstanceState::Stopped
    ));

    // Invalid transitions (Negative states)
    assert!(!can_transition(
        InstanceState::Suspended,
        InstanceState::Building
    ));
    assert!(!can_transition(
        InstanceState::Stopped,
        InstanceState::Suspended
    ));
    assert!(!can_transition(
        InstanceState::Building,
        InstanceState::Suspended
    ));
}
