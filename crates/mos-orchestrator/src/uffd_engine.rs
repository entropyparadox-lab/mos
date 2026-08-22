use anyhow::{Context, Result};
use mos_core::InstanceConfig;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tracing::{debug, info};

use crate::firecracker::{FirecrackerClient, FirecrackerProcess, MicroVmInstance};
use crate::snapshot_engine::SnapshotArtifacts;

#[derive(Debug, Clone)]
pub struct CompressedSnapshot {
    pub snapshot_path: PathBuf,
    pub mem_zstd_path: PathBuf,
    pub rootfs_path: PathBuf,
    pub original_size_bytes: u64,
    pub compressed_size_bytes: u64,
    pub compression_ratio: f64,
}

pub struct UffdSnapshotEngine {
    firecracker_bin: PathBuf,
}

impl UffdSnapshotEngine {
    pub fn new(firecracker_bin: PathBuf) -> Self {
        Self { firecracker_bin }
    }

    pub fn compress_snapshot(
        &self,
        artifacts: &SnapshotArtifacts,
        out_dir: &Path,
    ) -> Result<CompressedSnapshot> {
        let _ = std::fs::create_dir_all(out_dir);

        let mem_zstd_path = out_dir.join("memory.mem.zst");
        let input_file = File::open(&artifacts.mem_path)
            .with_context(|| format!("Failed to open memory file {:?}", artifacts.mem_path))?;
        let output_file = File::create(&mem_zstd_path)
            .with_context(|| format!("Failed to create zstd file {:?}", mem_zstd_path))?;

        let mut reader = BufReader::new(input_file);
        let mut writer = BufWriter::new(output_file);

        // ZSTD Level 3 (Fast, balanced for cloud VM snapshots)
        zstd::stream::copy_encode(&mut reader, &mut writer, 3)?;

        let original_size = std::fs::metadata(&artifacts.mem_path)?.len();
        let compressed_size = std::fs::metadata(&mem_zstd_path)?.len();
        let ratio = if original_size > 0 {
            compressed_size as f64 / original_size as f64
        } else {
            1.0
        };

        info!(
            original_mb = original_size / (1024 * 1024),
            compressed_mb = compressed_size / (1024 * 1024),
            ratio = format!("{:.2}%", ratio * 100.0),
            "ZSTD Snapshot compression complete"
        );

        Ok(CompressedSnapshot {
            snapshot_path: artifacts.snapshot_path.clone(),
            mem_zstd_path,
            rootfs_path: artifacts.rootfs_path.clone(),
            original_size_bytes: original_size,
            compressed_size_bytes: compressed_size,
            compression_ratio: ratio,
        })
    }

    pub fn decompress_snapshot(
        &self,
        compressed: &CompressedSnapshot,
        dest_mem_path: &Path,
    ) -> Result<SnapshotArtifacts> {
        let input_file = File::open(&compressed.mem_zstd_path)?;
        let output_file = File::create(dest_mem_path)?;

        let mut reader = BufReader::new(input_file);
        let mut writer = BufWriter::new(output_file);

        zstd::stream::copy_decode(&mut reader, &mut writer)?;

        Ok(SnapshotArtifacts {
            snapshot_path: compressed.snapshot_path.clone(),
            mem_path: dest_mem_path.to_path_buf(),
            rootfs_path: compressed.rootfs_path.clone(),
        })
    }

    pub async fn resume_with_uffd(
        &self,
        socket_path: PathBuf,
        config: InstanceConfig,
        artifacts: &SnapshotArtifacts,
    ) -> Result<(MicroVmInstance, Duration)> {
        let start = Instant::now();

        // 1. Spawn Firecracker process with UFFD handler support
        let process = FirecrackerProcess::spawn(&self.firecracker_bin, socket_path.clone()).await?;
        let client = FirecrackerClient::new(socket_path);

        // 2. Load snapshot directly and resume
        client
            .load_snapshot(&artifacts.snapshot_path, &artifacts.mem_path, true)
            .await?;

        let elapsed = start.elapsed();
        debug!(
            instance_id = %config.id,
            elapsed_us = elapsed.as_micros(),
            "MicroVM resumed via UFFD On-Demand engine"
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
