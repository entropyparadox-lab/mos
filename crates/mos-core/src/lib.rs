use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstanceId(pub Uuid);

impl InstanceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for InstanceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for InstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceState {
    Building,
    Starting,
    Running,
    Paused,
    Suspended,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceConfig {
    pub id: InstanceId,
    pub name: String,
    pub vcpu_count: u8,
    pub mem_size_mib: u32,
    pub kernel_path: PathBuf,
    pub rootfs_path: PathBuf,
    pub tap_device: Option<String>,
    pub guest_ip: Option<String>,
    pub guest_port: u16,
    pub created_at: DateTime<Utc>,
}

impl InstanceConfig {
    pub fn new(name: impl Into<String>, kernel: PathBuf, rootfs: PathBuf) -> Self {
        Self {
            id: InstanceId::new(),
            name: name.into(),
            vcpu_count: 1,
            mem_size_mib: 128,
            kernel_path: kernel,
            rootfs_path: rootfs,
            tap_device: None,
            guest_ip: None,
            guest_port: 8080,
            created_at: Utc::now(),
        }
    }
}

#[derive(Error, Debug)]
pub enum MosError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("MicroVM error: {0}")]
    Vm(String),

    #[error("HTTP proxy error: {0}")]
    Proxy(String),

    #[error("Build error: {0}")]
    Build(String),

    #[error("NotFound: {0}")]
    NotFound(String),
}

pub type MosResult<T> = Result<T, MosError>;

pub mod auth;
pub mod billing;
pub mod tenant;
pub mod volume;

pub use auth::{Ed25519AuthManager, RbacTokenPayload, Role};
pub use billing::{
    BillingEngine, BillingError, BillingRate, BillingTransaction, CreditAccount, UsageMetric,
};
pub use tenant::{QuotaError, TenantId, TenantManager, TenantNamespace};
pub use volume::{VolumeAccessMode, VolumeConfig, VolumeError, VolumeId, VolumeQuota};
