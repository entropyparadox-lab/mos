use hmac::{Hmac, Mac};
use m_os_edge::{EdgeRouter, RouteTarget, WebhookVerifier};
use mos_core::InstanceId;
use sha2::Sha256;

#[test]
fn test_weighted_canary_routing_distribution() {
    let router = EdgeRouter::new();
    let stable_id = InstanceId::new();
    let canary_id = InstanceId::new();

    let stable_target = RouteTarget {
        instance_id: stable_id,
        host: "127.0.0.1".to_string(),
        port: 8081,
        is_suspended: false,
    };
    let canary_target = RouteTarget {
        instance_id: canary_id,
        host: "127.0.0.1".to_string(),
        port: 8082,
        is_suspended: false,
    };

    router.register("app.mos.local", stable_target);

    // 1. Initial state: 100% stable
    let target = router.resolve("app.mos.local").unwrap();
    assert_eq!(target.instance_id, stable_id);

    // 2. Set Canary 20%
    router.set_canary("app.mos.local", canary_target.clone(), 20, "v2-canary");

    let mut canary_count = 0;
    let mut stable_count = 0;
    for _ in 0..100 {
        let resolved = router.resolve("app.mos.local").unwrap();
        if resolved.instance_id == canary_id {
            canary_count += 1;
        } else {
            stable_count += 1;
        }
    }
    assert_eq!(canary_count, 20);
    assert_eq!(stable_count, 80);

    // 3. Promote Canary to 50%
    router.promote_canary_step("app.mos.local", 50);
    canary_count = 0;
    for _ in 0..100 {
        let resolved = router.resolve("app.mos.local").unwrap();
        if resolved.instance_id == canary_id {
            canary_count += 1;
        }
    }
    assert_eq!(canary_count, 50);

    // 4. Promote Canary to 100% (Full cutover)
    router.promote_canary_step("app.mos.local", 100);
    for _ in 0..10 {
        let resolved = router.resolve("app.mos.local").unwrap();
        assert_eq!(resolved.instance_id, canary_id);
    }
}

#[test]
fn test_canary_instant_rollback() {
    let router = EdgeRouter::new();
    let stable_id = InstanceId::new();
    let canary_id = InstanceId::new();

    let stable_target = RouteTarget {
        instance_id: stable_id,
        host: "127.0.0.1".to_string(),
        port: 8081,
        is_suspended: false,
    };
    let canary_target = RouteTarget {
        instance_id: canary_id,
        host: "127.0.0.1".to_string(),
        port: 8082,
        is_suspended: false,
    };

    router.register("app.mos.local", stable_target);
    router.set_canary("app.mos.local", canary_target, 30, "v2-canary");

    // Instantly roll back
    router.rollback_canary("app.mos.local");

    for _ in 0..50 {
        let resolved = router.resolve("app.mos.local").unwrap();
        assert_eq!(resolved.instance_id, stable_id);
    }
}

#[test]
fn test_github_webhook_hmac_verification() {
    let secret = "mos_webhook_secret_key_12345";
    let body = br#"{"ref":"refs/heads/main","repository":{"full_name":"example-org/vibe-demo"},"head_commit":{"id":"abc1234567","author":{"username":"developer"},"message":"feat: new scale-to-zero model"}}"#;

    // Compute expected HMAC
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    let sig_bytes = mac.finalize().into_bytes();
    let sig_header = format!("sha256={}", hex::encode(sig_bytes));

    let valid = WebhookVerifier::verify_github_signature(secret, body, &sig_header);
    assert!(valid);

    let invalid = WebhookVerifier::verify_github_signature(secret, body, "sha256=invalidhex0000");
    assert!(!invalid);

    let payload = WebhookVerifier::parse_github_push_event(body).unwrap();
    assert_eq!(payload.repository, "example-org/vibe-demo");
    assert_eq!(payload.branch, "main");
    assert_eq!(payload.commit_sha, "abc1234567");
    assert_eq!(payload.author, "developer");
}
