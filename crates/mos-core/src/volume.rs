use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VolumeId(pub Uuid);

impl VolumeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for VolumeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for VolumeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VolumeAccessMode {
    ReadWriteOnce,
    ReadWriteMany,
    ReadOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeConfig {
    pub id: VolumeId,
    pub name: String,
    pub tenant_id: String,
    pub capacity_bytes: u64,
    pub used_bytes: u64,
    pub host_path: PathBuf,
    pub guest_mount_path: String,
    pub access_mode: VolumeAccessMode,
    pub created_at: DateTime<Utc>,
}

impl VolumeConfig {
    pub fn new(
        name: impl Into<String>,
        tenant_id: impl Into<String>,
        capacity_bytes: u64,
        host_path: PathBuf,
        guest_mount_path: impl Into<String>,
        access_mode: VolumeAccessMode,
    ) -> Self {
        Self {
            id: VolumeId::new(),
            name: name.into(),
            tenant_id: tenant_id.into(),
            capacity_bytes,
            used_bytes: 0,
            host_path,
            guest_mount_path: guest_mount_path.into(),
            access_mode,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeQuota {
    pub max_volumes: usize,
    pub max_total_storage_bytes: u64,
}

impl Default for VolumeQuota {
    fn default() -> Self {
        Self {
            max_volumes: 10,
            max_total_storage_bytes: 100 * 1024 * 1024 * 1024, // 100 GiB
        }
    }
}

#[derive(Error, Debug)]
pub enum VolumeError {
    #[error("Volume quota exceeded: max {max}, requested {requested}")]
    QuotaExceeded { max: u64, requested: u64 },

    #[error("Volume not found: {0}")]
    NotFound(String),

    #[error("Access denied for volume {volume_id} by tenant {tenant_id}")]
    AccessDenied {
        volume_id: String,
        tenant_id: String,
    },

    #[error("Volume {0} is already mounted exclusively")]
    ExclusiveLockConflict(String),

    #[error("I/O error: {0}")]
    Io(String),
}
