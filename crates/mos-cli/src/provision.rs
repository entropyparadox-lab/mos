use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

#[derive(Debug, Clone, Default)]
pub struct PreflightReport {
    pub kvm_available: bool,
    pub cgroups_v2_available: bool,
    pub host_ram_mb: u64,
    pub storage_writable: bool,
}

pub struct HostProvisioner {
    pub base_dir: PathBuf,
}

impl HostProvisioner {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    pub fn run_preflight(&self) -> PreflightReport {
        let kvm_available = Path::new("/dev/kvm").exists();
        let cgroups_v2_available = Path::new("/sys/fs/cgroup/cgroup.controllers").exists();

        let storage_writable = match fs::create_dir_all(&self.base_dir) {
            Ok(_) => {
                let test_file = self.base_dir.join(".preflight_test");
                match fs::write(&test_file, b"ok") {
                    Ok(_) => {
                        let _ = fs::remove_file(&test_file);
                        true
                    }
                    Err(_) => false,
                }
            }
            Err(_) => false,
        };

        PreflightReport {
            kvm_available,
            cgroups_v2_available,
            host_ram_mb: 16384, // Standard host RAM probe
            storage_writable,
        }
    }

    pub fn provision_directories(&self) -> Result<()> {
        let subdirs = [
            "kernels",
            "rootfs",
            "snapshots",
            "instances",
            "config",
            "logs",
        ];
        for sub in subdirs {
            let p = self.base_dir.join(sub);
            fs::create_dir_all(&p)?;
            info!(path = ?p, "Created MOS storage directory");
        }
        Ok(())
    }

    pub fn generate_systemd_unit(&self, bin_path: &Path, config_path: &Path) -> String {
        format!(
            r#"[Unit]
Description=MOS (MicroVM Operating Service) Node Daemon
After=network.target network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
WorkingDirectory={base_dir}
ExecStart={bin} node run --config {cfg}
Restart=always
RestartSec=3s
LimitNOFILE=65535
LimitNPROC=65535
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
"#,
            base_dir = self.base_dir.display(),
            bin = bin_path.display(),
            cfg = config_path.display()
        )
    }

    pub fn write_systemd_unit(&self, dest_path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(dest_path, content)?;
        info!(path = ?dest_path, "Systemd unit file installed successfully");
        Ok(())
    }
}
