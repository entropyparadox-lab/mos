use anyhow::Result;
use nix::mount::{mount, MsFlags};
use std::fs;
use std::path::Path;
use tracing::{debug, info, warn};

pub fn mount_early_filesystems() -> Result<()> {
    info!("🚀 [mos-init] Initializing early virtual filesystems...");

    let mounts: Vec<(&str, &str, &str, MsFlags)> = vec![
        (
            "/proc",
            "proc",
            "proc",
            MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC,
        ),
        (
            "/sys",
            "sysfs",
            "sysfs",
            MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC,
        ),
        ("/dev", "devtmpfs", "devtmpfs", MsFlags::MS_NOSUID),
        (
            "/run",
            "tmpfs",
            "tmpfs",
            MsFlags::MS_NOSUID | MsFlags::MS_NODEV,
        ),
        (
            "/tmp",
            "tmpfs",
            "tmpfs",
            MsFlags::MS_NOSUID | MsFlags::MS_NODEV,
        ),
    ];

    for (target, fstype, src, flags) in mounts {
        let path = Path::new(target);
        if !path.exists() {
            let _ = fs::create_dir_all(path);
        }

        match mount(Some(src), target, Some(fstype), flags, None::<&str>) {
            Ok(_) => debug!("Mounted {} ({}) at {}", src, fstype, target),
            Err(e) => {
                // In non-root testing environments, mount will fail with EPERM, which is expected.
                warn!(
                    "Could not mount {} at {} (ignoring if unprivileged/mock): {}",
                    src, target, e
                );
            }
        }
    }

    // Ensure /dev/pts exists for pseudo-terminals
    let devpts = Path::new("/dev/pts");
    if !devpts.exists() {
        let _ = fs::create_dir_all(devpts);
    }
    let _ = mount(
        Some("devpts"),
        "/dev/pts",
        Some("devpts"),
        MsFlags::MS_NOSUID | MsFlags::MS_NOEXEC,
        Some("gid=5,mode=620,ptmxmode=666"),
    );

    Ok(())
}
