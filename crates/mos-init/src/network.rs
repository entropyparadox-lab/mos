use anyhow::Result;
use std::process::Command;
use tracing::{info, warn};

pub fn configure_networking(guest_ip: Option<&str>, gateway: Option<&str>) -> Result<()> {
    info!("🌐 [mos-init] Configuring guest network interfaces...");

    // 1. Bring up loopback interface (lo)
    let _ = Command::new("ip")
        .args(["link", "set", "lo", "up"])
        .status();

    // 2. Configure eth0 if guest_ip is specified
    if let Some(ip) = guest_ip {
        info!("Setting eth0 address to {}", ip);
        let prefix = if ip.contains('/') {
            ip.to_string()
        } else {
            format!("{}/16", ip)
        };
        let status = Command::new("ip")
            .args(["addr", "add", &prefix, "dev", "eth0"])
            .status();
        if let Err(e) = status {
            warn!("ip addr add failed (ignoring if mock/no-net): {}", e);
        }

        let _ = Command::new("ip")
            .args(["link", "set", "eth0", "up"])
            .status();

        if let Some(gw) = gateway {
            info!("Setting default route via {}", gw);
            let _ = Command::new("ip")
                .args(["route", "add", "default", "via", gw, "dev", "eth0"])
                .status();
        }
    } else {
        // Try udhcpc if available
        let _ = Command::new("udhcpc")
            .args(["-i", "eth0", "-q", "-n"])
            .status();
    }

    Ok(())
}
