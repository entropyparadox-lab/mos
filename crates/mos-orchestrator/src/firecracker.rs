use anyhow::{bail, Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::Request;
use hyper_util::rt::TokioIo;
use mos_core::InstanceConfig;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tracing::{debug, info};

use crate::cgroup::CgroupController;
use crate::rate_limiter::RateLimiterConfig;
use crate::vsock::VsockDeviceConfig;

pub struct FirecrackerProcess {
    child: Child,
    pub socket_path: PathBuf,
}

impl FirecrackerProcess {
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    pub async fn spawn(firecracker_bin: &Path, socket_path: PathBuf) -> Result<Self> {
        if socket_path.exists() {
            let _ = tokio::fs::remove_file(&socket_path).await;
        }

        if let Some(parent) = socket_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        info!(
            bin = %firecracker_bin.display(),
            socket = %socket_path.display(),
            "Spawning Firecracker process"
        );

        let child = Command::new(firecracker_bin)
            .arg("--api-sock")
            .arg(&socket_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn firecracker binary")?;

        // Wait up to 2 seconds for the socket to appear
        let mut waited = Duration::ZERO;
        let step = Duration::from_millis(5);
        while !socket_path.exists() {
            tokio::time::sleep(step).await;
            waited += step;
            if waited > Duration::from_secs(2) {
                bail!(
                    "Timeout waiting for Firecracker API socket: {:?}",
                    socket_path
                );
            }
        }

        Ok(Self { child, socket_path })
    }

    pub async fn kill(&mut self) -> Result<()> {
        let _ = self.child.kill().await;
        if self.socket_path.exists() {
            let _ = tokio::fs::remove_file(&self.socket_path).await;
        }
        Ok(())
    }
}

pub struct FirecrackerClient {
    socket_path: PathBuf,
}

impl FirecrackerClient {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    async fn send_request(
        &self,
        method: hyper::Method,
        path: &str,
        body: serde_json::Value,
    ) -> Result<String> {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .with_context(|| format!("Failed to connect to socket {:?}", self.socket_path))?;
        let io = TokioIo::new(stream);

        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
        tokio::spawn(async move {
            if let Err(err) = conn.await {
                debug!("Connection error: {:?}", err);
            }
        });

        let body_bytes = serde_json::to_vec(&body)?;
        let req = Request::builder()
            .method(method)
            .uri(format!("http://localhost{}", path))
            .header("Host", "localhost")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(Full::new(Bytes::from(body_bytes)))?;

        let res = sender.send_request(req).await?;
        let status = res.status();
        let body_bytes = res.into_body().collect().await?.to_bytes();
        let body_str = String::from_utf8_lossy(&body_bytes).to_string();

        if !status.is_success() && status.as_u16() != 204 {
            bail!(
                "Firecracker API {} {} failed ({}): {}",
                path,
                status,
                status.as_u16(),
                body_str
            );
        }

        Ok(body_str)
    }

    pub async fn set_boot_source(&self, kernel_path: &Path, boot_args: &str) -> Result<()> {
        let body = serde_json::json!({
            "kernel_image_path": kernel_path.to_string_lossy(),
            "boot_args": boot_args
        });
        self.send_request(hyper::Method::PUT, "/boot-source", body)
            .await?;
        Ok(())
    }

    pub async fn set_root_drive(
        &self,
        rootfs_path: &Path,
        read_only: bool,
        rate_limiter: Option<&RateLimiterConfig>,
    ) -> Result<()> {
        let mut body = serde_json::json!({
            "drive_id": "rootfs",
            "path_on_host": rootfs_path.to_string_lossy(),
            "is_root_device": true,
            "is_read_only": read_only
        });

        if let Some(rl) = rate_limiter {
            body["rate_limiter"] = serde_json::to_value(rl)?;
        }

        self.send_request(hyper::Method::PUT, "/drives/rootfs", body)
            .await?;
        Ok(())
    }

    pub async fn set_machine_config(&self, vcpu_count: u8, mem_size_mib: u32) -> Result<()> {
        let body = serde_json::json!({
            "vcpu_count": vcpu_count,
            "mem_size_mib": mem_size_mib,
            "smt": false
        });
        self.send_request(hyper::Method::PUT, "/machine-config", body)
            .await?;
        Ok(())
    }

    pub async fn set_network_interface(
        &self,
        iface_id: &str,
        tap_name: &str,
        mac: &str,
        rate_limiter: Option<&RateLimiterConfig>,
    ) -> Result<()> {
        let mut body = serde_json::json!({
            "iface_id": iface_id,
            "guest_mac": mac,
            "host_dev_name": tap_name
        });

        if let Some(rl) = rate_limiter {
            body["rate_limiter"] = serde_json::to_value(rl)?;
        }

        self.send_request(
            hyper::Method::PUT,
            &format!("/network-interfaces/{}", iface_id),
            body,
        )
        .await?;
        Ok(())
    }

    pub async fn set_vsock(&self, config: &VsockDeviceConfig) -> Result<()> {
        let body = serde_json::json!({
            "vsock_id": config.vsock_id,
            "guest_cid": config.guest_cid,
            "uds_path": config.uds_path.to_string_lossy(),
        });
        self.send_request(hyper::Method::PUT, "/vsock", body)
            .await?;
        Ok(())
    }

    pub async fn start_instance(&self) -> Result<()> {
        let body = serde_json::json!({
            "action_type": "InstanceStart"
        });
        self.send_request(hyper::Method::PUT, "/actions", body)
            .await?;
        Ok(())
    }

    pub async fn pause(&self) -> Result<()> {
        let body = serde_json::json!({
            "state": "Paused"
        });
        self.send_request(hyper::Method::PATCH, "/vm", body).await?;
        Ok(())
    }

    pub async fn resume(&self) -> Result<()> {
        let body = serde_json::json!({
            "state": "Resumed"
        });
        self.send_request(hyper::Method::PATCH, "/vm", body).await?;
        Ok(())
    }

    pub async fn create_snapshot(&self, snapshot_path: &Path, mem_path: &Path) -> Result<()> {
        let body = serde_json::json!({
            "snapshot_type": "Full",
            "snapshot_path": snapshot_path.to_string_lossy(),
            "mem_file_path": mem_path.to_string_lossy()
        });
        self.send_request(hyper::Method::PUT, "/snapshot/create", body)
            .await?;
        Ok(())
    }

    pub async fn load_snapshot(
        &self,
        snapshot_path: &Path,
        mem_path: &Path,
        resume_vm: bool,
    ) -> Result<()> {
        let body = serde_json::json!({
            "snapshot_path": snapshot_path.to_string_lossy(),
            "mem_file_path": mem_path.to_string_lossy(),
            "enable_diff_snapshots": false,
            "resume_vm": resume_vm
        });
        self.send_request(hyper::Method::PUT, "/snapshot/load", body)
            .await?;
        Ok(())
    }
}

pub struct MicroVmInstance {
    pub config: InstanceConfig,
    pub process: FirecrackerProcess,
    pub client: FirecrackerClient,
    pub cgroup: Option<CgroupController>,
    pub vsock: Option<VsockDeviceConfig>,
}

impl MicroVmInstance {
    pub async fn boot(
        firecracker_bin: &Path,
        socket_path: PathBuf,
        config: InstanceConfig,
        boot_args: &str,
    ) -> Result<Self> {
        Self::boot_with_options(firecracker_bin, socket_path, config, boot_args, true, None).await
    }

    pub async fn boot_with_options(
        firecracker_bin: &Path,
        socket_path: PathBuf,
        config: InstanceConfig,
        boot_args: &str,
        enable_cgroup: bool,
        vsock_config: Option<VsockDeviceConfig>,
    ) -> Result<Self> {
        let process = FirecrackerProcess::spawn(firecracker_bin, socket_path.clone()).await?;
        let client = FirecrackerClient::new(socket_path);

        let mut cgroup = None;
        if enable_cgroup {
            if let Some(pid) = process.pid() {
                let mut cg = CgroupController::new(&config.id.to_string(), None);
                let _ = cg.setup(pid, config.vcpu_count, config.mem_size_mib);
                cgroup = Some(cg);
            }
        }

        client
            .set_boot_source(&config.kernel_path, boot_args)
            .await?;
        let disk_rl = RateLimiterConfig::disk_default();
        client
            .set_root_drive(&config.rootfs_path, false, Some(&disk_rl))
            .await?;
        client
            .set_machine_config(config.vcpu_count, config.mem_size_mib)
            .await?;

        if let Some(tap) = &config.tap_device {
            let net_rl = RateLimiterConfig::network_default();
            client
                .set_network_interface("eth0", tap, "AA:FC:00:00:00:02", Some(&net_rl))
                .await?;
        }

        if let Some(vsock) = &vsock_config {
            client.set_vsock(vsock).await?;
        }

        client.start_instance().await?;

        Ok(Self {
            config,
            process,
            client,
            cgroup,
            vsock: vsock_config,
        })
    }
}
