use crate::firecracker::{FirecrackerClient, FirecrackerProcess, MicroVmInstance};
use anyhow::Result;
use mos_core::InstanceConfig;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::info;

pub struct SnapshotArtifacts {
    pub snapshot_path: PathBuf,
    pub mem_path: PathBuf,
    pub rootfs_path: PathBuf,
}

pub struct SnapshotEngine {
    firecracker_bin: PathBuf,
}

impl SnapshotEngine {
    pub fn new(firecracker_bin: PathBuf) -> Self {
        Self { firecracker_bin }
    }

    /// Pause VM and create memory snapshot files
    pub async fn snapshot_and_stop(
        &self,
        mut instance: MicroVmInstance,
        snapshot_dir: &Path,
    ) -> Result<SnapshotArtifacts> {
        let instance_id = instance.config.id;
        tokio::fs::create_dir_all(snapshot_dir).await?;

        let snapshot_path = snapshot_dir.join(format!("{}.snap", instance_id));
        let mem_path = snapshot_dir.join(format!("{}.mem", instance_id));

        info!(instance_id = %instance_id, "Pausing MicroVM for snapshot...");
        instance.client.pause().await?;

        info!(instance_id = %instance_id, "Creating Firecracker snapshot...");
        instance
            .client
            .create_snapshot(&snapshot_path, &mem_path)
            .await?;

        // Stop the running firecracker process
        instance.process.kill().await?;

        Ok(SnapshotArtifacts {
            snapshot_path,
            mem_path,
            rootfs_path: instance.config.rootfs_path,
        })
    }

    /// Fast resume MicroVM from memory snapshot (< 30ms)
    pub async fn resume_from_snapshot(
        &self,
        socket_path: PathBuf,
        config: InstanceConfig,
        artifacts: &SnapshotArtifacts,
    ) -> Result<(MicroVmInstance, std::time::Duration)> {
        let start = Instant::now();

        // 1. Spawn a fresh Firecracker process
        let process = FirecrackerProcess::spawn(&self.firecracker_bin, socket_path.clone()).await?;
        let client = FirecrackerClient::new(socket_path);

        // 2. Load snapshot directly and resume
        client
            .load_snapshot(&artifacts.snapshot_path, &artifacts.mem_path, true)
            .await?;

        let elapsed = start.elapsed();
        info!(
            instance_id = %config.id,
            elapsed_ms = elapsed.as_millis(),
            "MicroVM resumed from snapshot successfully"
        );

        let instance = MicroVmInstance {
            config,
            process,
            client,
            cgroup: None,
            vsock: None,
        };

        Ok((instance, elapsed))
    }
}
