use dashmap::DashMap;
use mos_core::{InstanceId, VolumeAccessMode, VolumeConfig, VolumeError, VolumeId, VolumeQuota};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone)]
pub struct VolumeAttachment {
    pub volume_id: VolumeId,
    pub instance_id: InstanceId,
    pub mount_path: String,
    pub read_only: bool,
}

#[derive(Clone)]
pub struct VolumeManager {
    volumes: Arc<DashMap<VolumeId, VolumeConfig>>,
    attachments: Arc<DashMap<VolumeId, HashSet<InstanceId>>>,
    quotas: Arc<DashMap<String, VolumeQuota>>,
    base_storage_dir: PathBuf,
}

impl VolumeManager {
    pub fn new(base_storage_dir: impl Into<PathBuf>) -> Self {
        Self {
            volumes: Arc::new(DashMap::new()),
            attachments: Arc::new(DashMap::new()),
            quotas: Arc::new(DashMap::new()),
            base_storage_dir: base_storage_dir.into(),
        }
    }

    pub fn set_tenant_quota(&self, tenant_id: impl Into<String>, quota: VolumeQuota) {
        self.quotas.insert(tenant_id.into(), quota);
    }

    pub fn create_volume(
        &self,
        tenant_id: &str,
        name: &str,
        capacity_bytes: u64,
        guest_mount_path: &str,
        access_mode: VolumeAccessMode,
    ) -> Result<VolumeConfig, VolumeError> {
        let quota = self
            .quotas
            .get(tenant_id)
            .map(|r| r.clone())
            .unwrap_or_default();

        let current_volumes: Vec<_> = self
            .volumes
            .iter()
            .filter(|v| v.tenant_id == tenant_id)
            .map(|v| v.clone())
            .collect();

        if current_volumes.len() >= quota.max_volumes {
            return Err(VolumeError::QuotaExceeded {
                max: quota.max_volumes as u64,
                requested: (current_volumes.len() + 1) as u64,
            });
        }

        let total_storage: u64 = current_volumes.iter().map(|v| v.capacity_bytes).sum();
        if total_storage + capacity_bytes > quota.max_total_storage_bytes {
            return Err(VolumeError::QuotaExceeded {
                max: quota.max_total_storage_bytes,
                requested: total_storage + capacity_bytes,
            });
        }

        let host_path = self
            .base_storage_dir
            .join(format!("vol-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&host_path);

        let vol = VolumeConfig::new(
            name,
            tenant_id,
            capacity_bytes,
            host_path,
            guest_mount_path,
            access_mode,
        );

        self.volumes.insert(vol.id, vol.clone());
        info!(volume_id = %vol.id, name = %name, tenant = %tenant_id, "Created shared volume");

        Ok(vol)
    }

    pub fn attach_volume(
        &self,
        volume_id: &VolumeId,
        instance_id: &InstanceId,
        tenant_id: &str,
    ) -> Result<VolumeAttachment, VolumeError> {
        let vol = self
            .volumes
            .get(volume_id)
            .ok_or_else(|| VolumeError::NotFound(volume_id.to_string()))?;

        if vol.tenant_id != tenant_id {
            return Err(VolumeError::AccessDenied {
                volume_id: volume_id.to_string(),
                tenant_id: tenant_id.to_string(),
            });
        }

        let mut attached_instances = self.attachments.entry(*volume_id).or_default();

        match vol.access_mode {
            VolumeAccessMode::ReadWriteOnce => {
                if !attached_instances.is_empty() && !attached_instances.contains(instance_id) {
                    return Err(VolumeError::ExclusiveLockConflict(volume_id.to_string()));
                }
            }
            VolumeAccessMode::ReadWriteMany | VolumeAccessMode::ReadOnly => {}
        }

        attached_instances.insert(*instance_id);

        Ok(VolumeAttachment {
            volume_id: *volume_id,
            instance_id: *instance_id,
            mount_path: vol.guest_mount_path.clone(),
            read_only: vol.access_mode == VolumeAccessMode::ReadOnly,
        })
    }

    pub fn detach_volume(&self, volume_id: &VolumeId, instance_id: &InstanceId) -> bool {
        if let Some(mut set) = self.attachments.get_mut(volume_id) {
            set.remove(instance_id)
        } else {
            false
        }
    }

    pub fn list_volumes_by_tenant(&self, tenant_id: &str) -> Vec<VolumeConfig> {
        self.volumes
            .iter()
            .filter(|v| v.tenant_id == tenant_id)
            .map(|v| v.clone())
            .collect()
    }
}
