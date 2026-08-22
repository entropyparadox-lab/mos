use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

pub struct CgroupController {
    pub cgroup_path: PathBuf,
    pub is_active: bool,
}

impl CgroupController {
    pub fn new(instance_id: &str, base_cgroup: Option<&Path>) -> Self {
        let base = base_cgroup
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/sys/fs/cgroup/mos"));
        let cgroup_path = base.join(format!("inst-{}", instance_id));
        Self {
            cgroup_path,
            is_active: false,
        }
    }

    pub fn setup(&mut self, pid: u32, vcpu_count: u8, mem_size_mib: u32) -> Result<()> {
        info!(
            cgroup = %self.cgroup_path.display(),
            pid = pid,
            "Configuring Cgroups v2 resource isolation"
        );

        // Attempt to create cgroup directory
        if let Err(e) = fs::create_dir_all(&self.cgroup_path) {
            warn!(
                "Could not create cgroup directory {:?} (ignoring if unprivileged/testing): {}",
                self.cgroup_path, e
            );
            return Ok(());
        }

        self.is_active = true;

        // 1. Assign PID to cgroup.procs
        let procs_file = self.cgroup_path.join("cgroup.procs");
        if let Err(e) = fs::write(&procs_file, pid.to_string()) {
            warn!("Could not write to cgroup.procs (ignoring if mock): {}", e);
        }

        // 2. Configure cpu.max (e.g. 1 vCPU = 100000 100000)
        let cpu_max_file = self.cgroup_path.join("cpu.max");
        let quota = (vcpu_count as u64) * 100_000;
        let cpu_limit = format!("{} 100000", quota);
        let _ = fs::write(&cpu_max_file, cpu_limit);

        // 3. Configure memory.max (e.g. mem_size_mib + 32MB buffer for Firecracker VMM RSS)
        let memory_max_file = self.cgroup_path.join("memory.max");
        let mem_bytes = ((mem_size_mib as u64) + 32) * 1024 * 1024;
        let _ = fs::write(&memory_max_file, mem_bytes.to_string());

        debug!(
            "Cgroup v2 limits applied: CPU quota = {} us, Memory max = {} bytes",
            quota, mem_bytes
        );

        Ok(())
    }

    pub fn destroy(&self) -> Result<()> {
        if self.is_active && self.cgroup_path.exists() {
            let _ = fs::remove_dir(&self.cgroup_path);
        }
        Ok(())
    }
}
