use anyhow::Result;
use m_os_init::{
    mount::mount_early_filesystems,
    network::configure_networking,
    supervisor::{AppSupervisor, ProcessEvent},
    vsock_ipc::{current_epoch_secs, get_process_rss_bytes, GuestMessage, VsockIpcClient},
    InitConfig,
};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("🦀 [mos-init] Starting MOS Guest PID 1 Init Engine...");

    // 1. Mount essential virtual filesystems
    if let Err(e) = mount_early_filesystems() {
        warn!("Early filesystem mount notice: {}", e);
    }

    // 2. Load Init Configuration if present (/etc/mos/init.json)
    let config_path = Path::new("/etc/mos/init.json");
    let config: InitConfig = if config_path.exists() {
        match std::fs::read_to_string(config_path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => InitConfig::default(),
        }
    } else {
        InitConfig::default()
    };

    // 3. Configure Guest Networking
    if let Err(e) = configure_networking(config.guest_ip.as_deref(), config.gateway_ip.as_deref()) {
        warn!("Guest network configuration notice: {}", e);
    }

    // 4. If Litestream enabled, spawn background replication
    if config.enable_litestream {
        if let Some(litestream_conf) = &config.litestream_config_path {
            info!(
                "💾 [mos-init] Starting Litestream replication daemon with config {:?}",
                litestream_conf
            );
            let _ = tokio::process::Command::new("litestream")
                .args(["replicate", "-config", &litestream_conf.to_string_lossy()])
                .spawn();
        }
    }

    // 5. Spawn User Application
    let (event_tx, mut event_rx) = mpsc::channel::<ProcessEvent>(100);
    let mut supervisor = AppSupervisor::new();
    let mut child = supervisor
        .spawn(
            &config.app_cmd,
            &config.app_args,
            &config.env_vars,
            event_tx,
        )
        .await?;

    let mut ipc_client = VsockIpcClient::new_mock();
    let start_time = current_epoch_secs();

    // 6. Main Supervisor & Event Loop
    let mut ticker = tokio::time::interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let uptime = current_epoch_secs() - start_time;
                let rss = get_process_rss_bytes();
                let status = if supervisor.is_running() { "running" } else { "exited" };

                let hb = GuestMessage::Heartbeat {
                    status: status.to_string(),
                    memory_rss_bytes: rss,
                    uptime_secs: uptime,
                };
                let _ = ipc_client.send_message(&hb).await;

                // Reap any accumulated zombie child processes
                AppSupervisor::reap_zombies();
            }
            Some(event) = event_rx.recv() => {
                match event {
                    ProcessEvent::Started { pid } => {
                        info!("🚀 User application started with PID {}", pid);
                        let _ = ipc_client.send_message(&GuestMessage::ProcessState {
                            pid,
                            state: "running".to_string(),
                            exit_code: None,
                        }).await;
                    }
                    ProcessEvent::StdoutLine(line) => {
                        let _ = ipc_client.send_message(&GuestMessage::Log {
                            timestamp: current_epoch_secs(),
                            is_stderr: false,
                            line,
                        }).await;
                    }
                    ProcessEvent::StderrLine(line) => {
                        let _ = ipc_client.send_message(&GuestMessage::Log {
                            timestamp: current_epoch_secs(),
                            is_stderr: true,
                            line,
                        }).await;
                    }
                    ProcessEvent::Exited { code } => {
                        info!("App exited with code {:?}", code);
                        let _ = ipc_client.send_message(&GuestMessage::ProcessState {
                            pid: supervisor.child_pid().unwrap_or(0),
                            state: "exited".to_string(),
                            exit_code: code,
                        }).await;
                        break;
                    }
                }
            }
            status = child.wait() => {
                match status {
                    Ok(exit_status) => {
                        let code = exit_status.code();
                        info!("Child process finished with status: {:?}", exit_status);
                        let _ = ipc_client.send_message(&GuestMessage::ProcessState {
                            pid: supervisor.child_pid().unwrap_or(0),
                            state: "exited".to_string(),
                            exit_code: code,
                        }).await;
                    }
                    Err(e) => {
                        error!("Error waiting on child process: {}", e);
                    }
                }
                break;
            }
        }
    }

    info!("👋 [mos-init] Shutting down guest init...");
    Ok(())
}
