use anyhow::Result;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::info;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookPayload {
    pub repository: String,
    pub branch: String,
    pub commit_sha: String,
    pub author: String,
    pub commit_message: String,
}

pub struct WebhookVerifier;

impl WebhookVerifier {
    pub fn verify_github_signature(secret: &str, body: &[u8], signature_header: &str) -> bool {
        let signature_hex = if let Some(stripped) = signature_header.strip_prefix("sha256=") {
            stripped
        } else {
            signature_header
        };

        let expected_signature = match hex::decode(signature_hex) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };

        let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
            Ok(m) => m,
            Err(_) => return false,
        };

        mac.update(body);
        mac.verify_slice(&expected_signature).is_ok()
    }

    pub fn parse_github_push_event(json_body: &[u8]) -> Result<WebhookPayload> {
        let val: serde_json::Value = serde_json::from_slice(json_body)?;

        let repository = val
            .get("repository")
            .and_then(|r| r.get("full_name").or_else(|| r.get("name")))
            .and_then(|n| n.as_str())
            .unwrap_or("unknown/repo")
            .to_string();

        let branch = val
            .get("ref")
            .and_then(|r| r.as_str())
            .map(|r| r.trim_start_matches("refs/heads/"))
            .unwrap_or("main")
            .to_string();

        let head_commit = val.get("head_commit");
        let commit_sha = head_commit
            .and_then(|c| c.get("id"))
            .and_then(|i| i.as_str())
            .unwrap_or_else(|| {
                val.get("after")
                    .and_then(|a| a.as_str())
                    .unwrap_or("0000000000000000000000000000000000000000")
            })
            .to_string();

        let author = head_commit
            .and_then(|c| c.get("author"))
            .and_then(|a| a.get("username").or_else(|| a.get("name")))
            .and_then(|u| u.as_str())
            .unwrap_or("vibe-coder")
            .to_string();

        let commit_message = head_commit
            .and_then(|c| c.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("No message")
            .to_string();

        info!(
            repo = %repository,
            branch = %branch,
            sha = %commit_sha,
            "Received and parsed verified Git Webhook event"
        );

        Ok(WebhookPayload {
            repository,
            branch,
            commit_sha,
            author,
            commit_message,
        })
    }
}
