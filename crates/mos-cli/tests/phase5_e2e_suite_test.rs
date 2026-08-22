use mos_core::InstanceId;
use mos_edge::{
    tls::{TlsCertificateManager, TlsMode},
    EdgeRouter, RouteTarget, WebhookVerifier,
};

#[test]
fn test_phase5_e2e_tls_and_canary_pipeline() {
    // 1. TLS Engine Setup (Self-Signed Mode for local dev)
    let tls_mgr = TlsCertificateManager::new(TlsMode::SelfSigned {
        common_name: "*.mos.local".to_string(),
        cache_dir: std::path::PathBuf::from("/tmp/mos-tls"),
    });

    let cert = tls_mgr
        .resolve_cert("app.mos.local")
        .expect("Failed to resolve cert");
    assert!(cert.cert_pem.contains("COMMON_NAME=*.mos.local"));

    // 2. Subdomain Router with Weighted Canary
    let router = EdgeRouter::new();
    let stable_vm = InstanceId::new();
    let canary_vm = InstanceId::new();

    router.register(
        "app.mos.local",
        RouteTarget {
            instance_id: stable_vm,
            host: "127.0.0.1".to_string(),
            port: 8081,
            is_suspended: false,
        },
    );

    // Initial 100% stable
    let target = router.resolve("app.mos.local").unwrap();
    assert_eq!(target.instance_id, stable_vm);

    // Deploy Canary: 10% weight
    router.set_canary(
        "app.mos.local",
        RouteTarget {
            instance_id: canary_vm,
            host: "127.0.0.1".to_string(),
            port: 8082,
            is_suspended: false,
        },
        10,
        "v2-canary-git-sha",
    );

    let mut canary_hits = 0;
    for _ in 0..100 {
        if router.resolve("app.mos.local").unwrap().instance_id == canary_vm {
            canary_hits += 1;
        }
    }
    assert_eq!(canary_hits, 10);

    // Promote to 50%
    router.promote_canary_step("app.mos.local", 50);
    canary_hits = 0;
    for _ in 0..100 {
        if router.resolve("app.mos.local").unwrap().instance_id == canary_vm {
            canary_hits += 1;
        }
    }
    assert_eq!(canary_hits, 50);

    // Promote to 100% (Full cutover)
    router.promote_canary_step("app.mos.local", 100);
    let final_target = router.resolve("app.mos.local").unwrap();
    assert_eq!(final_target.instance_id, canary_vm);

    // 3. Webhook Simulation
    let secret = "vibe_webhook_secret_key";
    let body = br#"{"ref":"refs/heads/main","repository":{"full_name":"entropyparadox-lab/vibe-demo"},"head_commit":{"id":"feedbeef12","author":{"username":"octocat"},"message":"feat: zero config microvm deploy"}}"#;

    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    let sig_header = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

    let is_valid = WebhookVerifier::verify_github_signature(secret, body, &sig_header);
    assert!(is_valid);

    let parsed = WebhookVerifier::parse_github_push_event(body).unwrap();
    assert_eq!(parsed.commit_sha, "feedbeef12");
}
