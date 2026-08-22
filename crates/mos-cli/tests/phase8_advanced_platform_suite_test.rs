use mos_core::{
    BillingEngine, BillingRate, CreditAccount, InstanceId, VolumeAccessMode, VolumeQuota,
};
use mos_edge::{
    CanaryPipelineConfig, CanaryPipelineManager, EdgeRouter, PipelineEvaluation, RouteTarget,
    WebhookVerifier,
};
use mos_orchestrator::{UsageTracker, VolumeManager};
use std::thread::sleep;
use std::time::Duration;

#[test]
fn test_phase8_advanced_platform_full_pipeline() {
    let temp_dir = tempfile::tempdir().expect("Failed creating tempdir");

    // =========================================================================
    // 1. Distributed Shared Volume & Quota Isolation
    // =========================================================================
    let vol_mgr = VolumeManager::new(temp_dir.path());
    let quota = VolumeQuota {
        max_volumes: 3,
        max_total_storage_bytes: 20 * 1024 * 1024 * 1024, // 20 GiB
    };
    vol_mgr.set_tenant_quota("tenant-enterprise-x", quota);

    // Create a 5GB ReadWriteMany shared models volume
    let vol_shared = vol_mgr
        .create_volume(
            "tenant-enterprise-x",
            "ai-weights-volume",
            5 * 1024 * 1024 * 1024,
            "/mnt/models",
            VolumeAccessMode::ReadWriteMany,
        )
        .expect("Creating shared volume should succeed");

    let inst_worker_1 = InstanceId::new();
    let inst_worker_2 = InstanceId::new();

    // Attach to both workers (ReadWriteMany allows multiple)
    let attach_1 = vol_mgr
        .attach_volume(&vol_shared.id, &inst_worker_1, "tenant-enterprise-x")
        .expect("Attach to worker 1 should succeed");
    let attach_2 = vol_mgr
        .attach_volume(&vol_shared.id, &inst_worker_2, "tenant-enterprise-x")
        .expect("Attach to worker 2 should succeed");

    assert_eq!(attach_1.mount_path, "/mnt/models");
    assert_eq!(attach_2.mount_path, "/mnt/models");

    // Cross-tenant access must be rejected
    let alien_attach = vol_mgr.attach_volume(&vol_shared.id, &inst_worker_1, "tenant-attacker");
    assert!(alien_attach.is_err());
    assert!(format!("{}", alien_attach.unwrap_err()).contains("Access denied"));

    // =========================================================================
    // 2. Real-time Metered Usage Tracking & Credit Billing Engine
    // =========================================================================
    let billing = BillingEngine::new(BillingRate::default());
    billing.register_account(CreditAccount::new("tenant-enterprise-x", 100.0)); // 100 credits

    let tracker = UsageTracker::new(billing.clone());
    // Start tracking worker 1: 4 vCPU, 8192 MB RAM, 16384 MB VRAM
    tracker.start_tracking(inst_worker_1, "tenant-enterprise-x", 4, 8192, 16384);

    sleep(Duration::from_millis(50));
    tracker.record_egress(&inst_worker_1, 50 * 1024 * 1024); // 50MB egress

    let charge_results = tracker.tick_and_charge();
    assert_eq!(charge_results.len(), 1);
    let (tenant, res) = &charge_results[0];
    assert_eq!(tenant, "tenant-enterprise-x");
    let remaining_credits = res.as_ref().unwrap();
    assert!(*remaining_credits < 100.0);
    assert!(*remaining_credits > 99.0);

    let metric = tracker.get_tenant_metric("tenant-enterprise-x");
    assert!(metric.vcpu_seconds > 0.0);
    assert_eq!(metric.egress_bytes, 50 * 1024 * 1024);
    tracker.stop_tracking(&inst_worker_1);

    // =========================================================================
    // 3. GitOps Push Webhook & 3-Stage Progressive Canary Promotion
    // =========================================================================
    let webhook_secret = "k_sec_production_secret_9981";
    let push_json = br#"{
        "ref": "refs/heads/main",
        "repository": { "full_name": "vibe-corp/ai-app" },
        "head_commit": {
            "id": "7b8f9e1234567890abcdef1234567890abcdef12",
            "message": "feat: upgrade AI inference engine to v2",
            "author": { "name": "vibe-dev" }
        }
    }"#;

    // Generate HMAC signature
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(webhook_secret.as_bytes()).unwrap();
    mac.update(push_json);
    let signature = hex::encode(mac.finalize().into_bytes());

    assert!(WebhookVerifier::verify_github_signature(
        webhook_secret,
        push_json,
        &format!("sha256={}", signature)
    ));

    let parsed_event = WebhookVerifier::parse_github_push_event(push_json).unwrap();
    assert_eq!(parsed_event.branch, "main");
    assert_eq!(
        parsed_event.commit_sha,
        "7b8f9e1234567890abcdef1234567890abcdef12"
    );

    // Router & Canary Pipeline
    let router = EdgeRouter::new();
    let stable_target = RouteTarget::new(inst_worker_1, "172.16.0.2", 8080, false);
    router.register("ai-app.mos.local", stable_target);

    let canary_target = RouteTarget::new(inst_worker_2, "172.16.0.3", 8080, false);

    let canary_config = CanaryPipelineConfig {
        step_weights: vec![10, 50, 100],
        min_requests_per_step: 10,
        max_error_rate_percent: 5.0,
    };
    let pipeline = CanaryPipelineManager::new(router.clone(), canary_config);

    // Step 1: Start Canary (10%)
    pipeline.start_canary_deployment(
        "ai-app.mos.local",
        canary_target,
        &parsed_event.commit_sha[..7],
    );
    let routes = router.inspect_routes("ai-app.mos.local").unwrap();
    assert_eq!(routes.canary.as_ref().unwrap().weight, 10);
    assert_eq!(routes.stable.weight, 90);

    // Step 2: 10 healthy requests -> Advance to 50%
    for _ in 0..10 {
        pipeline.record_result("ai-app.mos.local", false);
    }
    let eval_step1 = pipeline.evaluate_and_advance("ai-app.mos.local");
    assert_eq!(
        eval_step1,
        PipelineEvaluation::Promoted {
            new_step: 1,
            new_weight: 50
        }
    );

    let routes = router.inspect_routes("ai-app.mos.local").unwrap();
    assert_eq!(routes.canary.as_ref().unwrap().weight, 50);

    // Step 3: 10 healthy requests -> 100% Full Production Promotion
    for _ in 0..10 {
        pipeline.record_result("ai-app.mos.local", false);
    }
    let eval_step2 = pipeline.evaluate_and_advance("ai-app.mos.local");
    assert_eq!(
        eval_step2,
        PipelineEvaluation::FullyPromoted {
            version_tag: parsed_event.commit_sha[..7].to_string()
        }
    );

    let routes = router.inspect_routes("ai-app.mos.local").unwrap();
    assert_eq!(routes.stable.version_tag, parsed_event.commit_sha[..7]);
    assert!(routes.canary.is_none());
}
