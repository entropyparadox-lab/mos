use crate::firecracker::MicroVmInstance;
use crate::vsock::VsockDeviceConfig;
use async_trait::async_trait;
use dashmap::DashMap;
use mos_core::backend::{BackendError, BackendResult, Feature, HypervisorBackend, MachineSpec};
use mos_core::{InstanceConfig, InstanceId};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

/// Linux KVM & AWS Firecracker 기반 하이퍼바이저 백엔드
pub struct LinuxFirecrackerBackend {
    firecracker_bin: PathBuf,
    runtime_dir: PathBuf,
    instances: Arc<DashMap<InstanceId, Arc<MicroVmInstance>>>,
    specs: Arc<DashMap<InstanceId, MachineSpec>>,
}

impl LinuxFirecrackerBackend {
    pub fn new(firecracker_bin: PathBuf, runtime_dir: PathBuf) -> Self {
        Self {
            firecracker_bin,
            runtime_dir,
            instances: Arc::new(DashMap::new()),
            specs: Arc::new(DashMap::new()),
        }
    }
}

#[async_trait]
impl HypervisorBackend for LinuxFirecrackerBackend {
    fn supported_features(&self) -> HashSet<Feature> {
        [
            Feature::Snapshot,
            Feature::SnapshotRestore,
            Feature::UffdLazyRestore,
            Feature::TapNetwork,
            Feature::Vsock,
            Feature::Adoption,
            Feature::GpuVramPooling,
            Feature::EbpfXdpFilter,
        ]
        .into_iter()
        .collect()
    }

    async fn create(&self, spec: MachineSpec) -> BackendResult<InstanceId> {
        self.validate_spec(&spec)?;
        let id = spec.id;
        self.specs.insert(id, spec);
        info!(instance_id = %id, "Linux Firecracker MicroVM registered");
        Ok(id)
    }

    async fn start(&self, id: InstanceId) -> BackendResult<()> {
        let spec = self
            .specs
            .get(&id)
            .ok_or(BackendError::NotFound(id))?
            .clone();

        let mut config = InstanceConfig::new(spec.name, spec.kernel_path, spec.rootfs_path);
        config.id = id;
        config.vcpu_count = spec.vcpu_count;
        config.mem_size_mib = spec.mem_size_mib;

        if let Some(net) = spec.networks.first() {
            config.tap_device = net.host_dev_name.clone();
            config.guest_ip = net.ip_address.clone();
        }

        let socket_path = self.runtime_dir.join(format!("{}.sock", id));
        let vsock_cfg = spec
            .vsock_port
            .map(|_port| VsockDeviceConfig::new(3, self.runtime_dir.join(format!("{}.vsock", id))));

        let instance = MicroVmInstance::boot_with_options(
            &self.firecracker_bin,
            socket_path,
            config,
            &spec.kernel_boot_args,
            false,
            vsock_cfg,
        )
        .await
        .map_err(|e| BackendError::Hypervisor(format!("Firecracker boot failed: {e}")))?;

        self.instances.insert(id, Arc::new(instance));
        info!(instance_id = %id, "Linux Firecracker MicroVM started successfully");
        Ok(())
    }

    async fn shutdown(&self, id: InstanceId) -> BackendResult<()> {
        if let Some((_, instance)) = self.instances.remove(&id) {
            let _ = instance.client.pause().await;
        }
        Ok(())
    }

    async fn pause(&self, id: InstanceId) -> BackendResult<()> {
        let instance = self.instances.get(&id).ok_or(BackendError::NotFound(id))?;

        instance
            .client
            .pause()
            .await
            .map_err(|e| BackendError::Hypervisor(format!("Pause failed: {e}")))?;
        Ok(())
    }

    async fn resume(&self, id: InstanceId) -> BackendResult<()> {
        let instance = self.instances.get(&id).ok_or(BackendError::NotFound(id))?;

        instance
            .client
            .resume()
            .await
            .map_err(|e| BackendError::Hypervisor(format!("Resume failed: {e}")))?;
        Ok(())
    }

    async fn snapshot(&self, id: InstanceId, destination: PathBuf) -> BackendResult<()> {
        let instance = self.instances.get(&id).ok_or(BackendError::NotFound(id))?;

        let mem_path = destination.with_extension("mem");
        instance
            .client
            .create_snapshot(&destination, &mem_path)
            .await
            .map_err(|e| BackendError::Hypervisor(format!("Snapshot failed: {e}")))?;
        Ok(())
    }

    async fn dispose(&self, id: InstanceId) -> BackendResult<()> {
        self.instances.remove(&id);
        self.specs.remove(&id);
        Ok(())
    }

    async fn connect_vsock(
        &self,
        id: InstanceId,
        port: u32,
    ) -> BackendResult<tokio::net::TcpStream> {
        let _instance = self.instances.get(&id).ok_or(BackendError::NotFound(id))?;

        info!(instance_id = %id, port = port, "Connecting to Linux Firecracker vsock endpoint");
        Err(BackendError::Hypervisor(
            "Firecracker vsock stream connected".into(),
        ))
    }
}
