use mos_core::InstanceId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug, PartialEq)]
pub enum GpuPoolError {
    #[error("No available GPU device with requested VRAM {requested_mib}MB")]
    InsufficientVram { requested_mib: u64 },
    #[error("GPU device not found for instance {0}")]
    InstanceNotBound(InstanceId),
    #[error("GPU device {0} is currently busy")]
    DeviceBusy(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDevice {
    pub index: u32,
    pub name: String,
    pub pci_address: String,
    pub total_vram_mib: u64,
    pub allocated_vram_mib: u64,
    pub bound_instance_id: Option<InstanceId>,
    pub is_scale_to_zero_idle: bool,
}

impl GpuDevice {
    pub fn new(
        index: u32,
        name: impl Into<String>,
        pci_address: impl Into<String>,
        total_vram_mib: u64,
    ) -> Self {
        Self {
            index,
            name: name.into(),
            pci_address: pci_address.into(),
            total_vram_mib,
            allocated_vram_mib: 0,
            bound_instance_id: None,
            is_scale_to_zero_idle: true,
        }
    }

    pub fn available_vram_mib(&self) -> u64 {
        self.total_vram_mib.saturating_sub(self.allocated_vram_mib)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuBinding {
    pub gpu_index: u32,
    pub pci_address: String,
    pub allocated_vram_mib: u64,
}

#[derive(Clone, Default)]
pub struct GpuPoolManager {
    devices: Arc<RwLock<HashMap<u32, GpuDevice>>>,
    instance_allocations: Arc<RwLock<HashMap<InstanceId, (u32, u64)>>>, // InstanceId -> (GPU index, VRAM MB)
}

impl GpuPoolManager {
    pub fn new() -> Self {
        Self {
            devices: Arc::new(RwLock::new(HashMap::new())),
            instance_allocations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register_device(&self, device: GpuDevice) {
        let mut map = self.devices.write().unwrap();
        map.insert(device.index, device);
    }

    pub fn bind_gpu_to_instance(
        &self,
        instance_id: &InstanceId,
        required_vram_mib: u64,
    ) -> Result<GpuBinding, GpuPoolError> {
        let mut devices = self.devices.write().unwrap();
        let mut allocations = self.instance_allocations.write().unwrap();

        // 1. Check if instance already has an active binding
        if let Some(&(gpu_idx, vram)) = allocations.get(instance_id) {
            if let Some(dev) = devices.get_mut(&gpu_idx) {
                dev.is_scale_to_zero_idle = false;
                return Ok(GpuBinding {
                    gpu_index: dev.index,
                    pci_address: dev.pci_address.clone(),
                    allocated_vram_mib: vram,
                });
            }
        }

        // 2. Find a GPU device with sufficient available VRAM
        for dev in devices.values_mut() {
            if dev.available_vram_mib() >= required_vram_mib {
                dev.allocated_vram_mib += required_vram_mib;
                dev.bound_instance_id = Some(*instance_id);
                dev.is_scale_to_zero_idle = false;

                allocations.insert(*instance_id, (dev.index, required_vram_mib));

                info!(
                    instance_id = %instance_id,
                    gpu_index = dev.index,
                    vram_mib = required_vram_mib,
                    "GPU allocated and bound to AI MicroVM"
                );

                return Ok(GpuBinding {
                    gpu_index: dev.index,
                    pci_address: dev.pci_address.clone(),
                    allocated_vram_mib: required_vram_mib,
                });
            }
        }

        Err(GpuPoolError::InsufficientVram {
            requested_mib: required_vram_mib,
        })
    }

    pub fn scale_to_zero_detach(&self, instance_id: &InstanceId) -> Result<u64, GpuPoolError> {
        let mut devices = self.devices.write().unwrap();
        let mut allocations = self.instance_allocations.write().unwrap();

        let (gpu_idx, freed_vram) = allocations
            .remove(instance_id)
            .ok_or(GpuPoolError::InstanceNotBound(*instance_id))?;

        if let Some(dev) = devices.get_mut(&gpu_idx) {
            dev.allocated_vram_mib = dev.allocated_vram_mib.saturating_sub(freed_vram);
            if dev.allocated_vram_mib == 0 {
                dev.bound_instance_id = None;
                dev.is_scale_to_zero_idle = true;
            }

            info!(
                instance_id = %instance_id,
                gpu_index = dev.index,
                freed_vram_mib = freed_vram,
                "MicroVM idle: GPU VRAM released (Scale-to-Zero VRAM 0MB)"
            );

            Ok(freed_vram)
        } else {
            Err(GpuPoolError::InstanceNotBound(*instance_id))
        }
    }

    pub fn total_vram_in_use_mib(&self) -> u64 {
        let devices = self.devices.read().unwrap();
        devices.values().map(|d| d.allocated_vram_mib).sum()
    }
}
