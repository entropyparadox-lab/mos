use m_os_edge::tls::{TlsCertificateManager, TlsConfig, TlsMode};
use std::path::PathBuf;

#[test]
fn test_tls_config_modes() {
    let self_signed = TlsConfig {
        mode: TlsMode::SelfSigned {
            common_name: "*.mos.local".to_string(),
            cache_dir: PathBuf::from("/tmp/tls"),
        },
        https_port: 8443,
        http_port: 8080,
        redirect_http_to_https: true,
    };
    assert_eq!(self_signed.https_port, 8443);
    assert!(self_signed.redirect_http_to_https);

    let offloaded = TlsConfig {
        mode: TlsMode::Offloaded,
        https_port: 443,
        http_port: 80,
        redirect_http_to_https: false,
    };
    assert_eq!(offloaded.mode, TlsMode::Offloaded);
}

#[test]
fn test_tls_certificate_resolution_and_sni() {
    let mgr = TlsCertificateManager::new(TlsMode::SelfSigned {
        common_name: "*.mos.local".to_string(),
        cache_dir: PathBuf::from("/tmp/tls"),
    });

    mgr.register_cert(
        "app.custom.com",
        "CERT_CUSTOM".to_string(),
        "KEY_CUSTOM".to_string(),
    );

    // Direct SNI match
    let custom = mgr.resolve_cert("app.custom.com");
    assert!(custom.is_some());
    assert_eq!(custom.unwrap().cert_pem, "CERT_CUSTOM");

    // Fallback self-signed for *.mos.local
    let fallback = mgr.resolve_cert("vibe-demo.mos.local");
    assert!(fallback.is_some());
    assert!(fallback
        .unwrap()
        .cert_pem
        .contains("COMMON_NAME=*.mos.local"));
}

#[test]
fn test_acme_http01_challenge_store() {
    let mgr = TlsCertificateManager::new(TlsMode::AutoAcme {
        contact_email: "admin@mos.dev".to_string(),
        staging: true,
        cache_dir: PathBuf::from("/tmp/acme"),
    });

    mgr.register_acme_challenge("test_token_123", "test_token_123.auth_fingerprint");
    let key_auth = mgr.get_acme_challenge("test_token_123");
    assert_eq!(
        key_auth,
        Some("test_token_123.auth_fingerprint".to_string())
    );

    let nonexistent = mgr.get_acme_challenge("unknown_token");
    assert!(nonexistent.is_none());
}
