use anyhow::{anyhow, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Developer,
    Viewer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RbacTokenPayload {
    pub token_id: String,
    pub tenant_id: String,
    pub role: Role,
    pub issued_at: u64,
    pub expires_at: u64,
}

pub struct Ed25519AuthManager {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl Ed25519AuthManager {
    pub fn new_random() -> Self {
        let mut csprng = rand_core::OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        Self {
            signing_key,
            verifying_key,
        }
    }

    pub fn from_seed(seed_32_bytes: &[u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(seed_32_bytes);
        let verifying_key = signing_key.verifying_key();
        Self {
            signing_key,
            verifying_key,
        }
    }

    pub fn verifying_key_hex(&self) -> String {
        hex::encode(self.verifying_key.to_bytes())
    }

    /// Sign payload into format: `<payload_base64>.<hex_signature>`
    pub fn sign_token(&self, payload: &RbacTokenPayload) -> Result<String> {
        let json = serde_json::to_string(payload)?;
        let payload_b64 = base64_encode(json.as_bytes());
        let sig: Signature = self.signing_key.sign(payload_b64.as_bytes());
        let sig_hex = hex::encode(sig.to_bytes());
        Ok(format!("{}.{}", payload_b64, sig_hex))
    }

    /// Verify token string and return payload if valid and unexpired
    pub fn verify_token(&self, token_str: &str) -> Result<RbacTokenPayload> {
        let parts: Vec<&str> = token_str.split('.').collect();
        if parts.len() != 2 {
            return Err(anyhow!("Invalid token format, expected <payload>.<sig>"));
        }

        let payload_b64 = parts[0];
        let sig_bytes = hex::decode(parts[1])?;
        if sig_bytes.len() != 64 {
            return Err(anyhow!("Invalid signature length"));
        }

        let mut sig_arr = [0u8; 64];
        sig_arr.copy_from_slice(&sig_bytes);
        let signature = Signature::from_bytes(&sig_arr);

        self.verifying_key
            .verify(payload_b64.as_bytes(), &signature)
            .map_err(|e| anyhow!("Ed25519 signature verification failed: {}", e))?;

        let json_bytes = base64_decode(payload_b64)?;
        let payload: RbacTokenPayload = serde_json::from_slice(&json_bytes)?;

        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        if payload.expires_at < now {
            return Err(anyhow!("Token has expired"));
        }

        Ok(payload)
    }
}

fn base64_encode(input: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

        out.push(CHARSET[(b0 >> 2) as usize] as char);
        out.push(CHARSET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARSET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(CHARSET[(b2 & 0x3f) as usize] as char);
        }
    }
    out
}

fn base64_decode(input: &str) -> Result<Vec<u8>> {
    let mut decode_map = [255u8; 256];
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    for (i, &b) in CHARSET.iter().enumerate() {
        decode_map[b as usize] = i as u8;
    }

    let input_bytes = input.as_bytes();
    let mut out = Vec::new();

    for chunk in input_bytes.chunks(4) {
        let v0 = decode_map[chunk[0] as usize];
        let v1 = if chunk.len() > 1 {
            decode_map[chunk[1] as usize]
        } else {
            0
        };
        let v2 = if chunk.len() > 2 {
            decode_map[chunk[2] as usize]
        } else {
            0
        };
        let v3 = if chunk.len() > 3 {
            decode_map[chunk[3] as usize]
        } else {
            0
        };

        if v0 == 255 || (chunk.len() > 1 && v1 == 255) {
            return Err(anyhow!("Invalid character in base64 string"));
        }

        let b0 = (v0 << 2) | (v1 >> 4);
        out.push(b0);

        if chunk.len() > 2 && v2 != 255 {
            let b1 = ((v1 & 0x0f) << 4) | (v2 >> 2);
            out.push(b1);
        }
        if chunk.len() > 3 && v3 != 255 {
            let b2 = ((v2 & 0x03) << 6) | v3;
            out.push(b2);
        }
    }
    Ok(out)
}
