use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum TlsMode {
    AutoAcme {
        contact_email: String,
        staging: bool,
        cache_dir: PathBuf,
    },
    SelfSigned {
        common_name: String,
        cache_dir: PathBuf,
    },
    Offloaded,
}

impl Default for TlsMode {
    fn default() -> Self {
        Self::SelfSigned {
            common_name: "*.mos.local".to_string(),
            cache_dir: PathBuf::from("/tmp/mos-tls"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub mode: TlsMode,
    pub https_port: u16,
    pub http_port: u16,
    pub redirect_http_to_https: bool,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            mode: TlsMode::default(),
            https_port: 443,
            http_port: 80,
            redirect_http_to_https: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TlsCertificate {
    pub cert_pem: String,
    pub key_pem: String,
    pub domain: String,
}

#[derive(Clone, Default)]
pub struct TlsCertificateManager {
    mode: TlsMode,
    certs: Arc<DashMap<String, TlsCertificate>>,
    acme_challenges: Arc<DashMap<String, String>>, // token -> key_authorization
}

impl TlsCertificateManager {
    pub fn new(mode: TlsMode) -> Self {
        Self {
            mode,
            certs: Arc::new(DashMap::new()),
            acme_challenges: Arc::new(DashMap::new()),
        }
    }

    pub fn mode(&self) -> &TlsMode {
        &self.mode
    }

    pub fn register_acme_challenge(&self, token: &str, key_auth: &str) {
        self.acme_challenges
            .insert(token.to_string(), key_auth.to_string());
    }

    pub fn get_acme_challenge(&self, token: &str) -> Option<String> {
        self.acme_challenges.get(token).map(|v| v.value().clone())
    }

    pub fn register_cert(&self, domain: impl Into<String>, cert_pem: String, key_pem: String) {
        let domain_str = domain.into();
        self.certs.insert(
            domain_str.clone(),
            TlsCertificate {
                cert_pem,
                key_pem,
                domain: domain_str,
            },
        );
    }

    pub fn resolve_cert(&self, sni_domain: &str) -> Option<TlsCertificate> {
        // Direct match
        if let Some(cert) = self.certs.get(sni_domain) {
            return Some(cert.value().clone());
        }

        // Wildcard match (*.mos.local or *.mos.dev)
        if let Some(dot_idx) = sni_domain.find('.') {
            let wildcard = format!("*{}", &sni_domain[dot_idx..]);
            if let Some(cert) = self.certs.get(&wildcard) {
                return Some(cert.value().clone());
            }
        }

        // Fallback default if in SelfSigned mode
        if let TlsMode::SelfSigned { common_name, .. } = &self.mode {
            return Some(self.generate_ephemeral_self_signed(common_name));
        }

        None
    }

    pub fn generate_ephemeral_self_signed(&self, common_name: &str) -> TlsCertificate {
        debug!(
            "Generating ephemeral self-signed certificate for {}",
            common_name
        );
        // Mock self-signed PEM certificate representation for test and local dev
        let mock_cert = format!(
            "-----BEGIN CERTIFICATE-----\nMIIBojCCAUqgAwIBAgIU...\nCOMMON_NAME={}\n-----END CERTIFICATE-----",
            common_name
        );
        let mock_key =
            "-----BEGIN PRIVATE KEY-----\nMIGHAgEAMBMGByqGSM49AgEG...\n-----END PRIVATE KEY-----"
                .to_string();

        TlsCertificate {
            cert_pem: mock_cert,
            key_pem: mock_key,
            domain: common_name.to_string(),
        }
    }
}
