use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tracing::debug;

pub const VSOCK_PORT_TELEMETRY: u32 = 52;
pub const VSOCK_PORT_LOGS: u32 = 53;
pub const VSOCK_PORT_CONTROL: u32 = 54;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GuestMessage {
    Heartbeat {
        status: String,
        memory_rss_bytes: u64,
        uptime_secs: u64,
    },
    Ready {
        port: u16,
        wake_latency_ms: u64,
    },
    Log {
        timestamp: u64,
        is_stderr: bool,
        line: String,
    },
    ProcessState {
        pid: u32,
        state: String,
        exit_code: Option<i32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostCommand {
    Ping,
    TimeSync { epoch_nanos: u64 },
    WarmConnections,
    Signal { signal: String },
    Shutdown,
}

pub struct VsockIpcClient {
    stream: Option<UnixStream>,
}

impl VsockIpcClient {
    pub async fn connect_uds(path: &std::path::Path) -> Result<Self> {
        let stream = UnixStream::connect(path).await?;
        Ok(Self {
            stream: Some(stream),
        })
    }

    pub fn new_mock() -> Self {
        Self { stream: None }
    }

    pub async fn send_message(&mut self, msg: &GuestMessage) -> Result<()> {
        let mut json = serde_json::to_string(msg)?;
        json.push('\n');

        if let Some(stream) = &mut self.stream {
            stream.write_all(json.as_bytes()).await?;
            stream.flush().await?;
        } else {
            debug!("[Mock Vsock IPC] Sent: {}", json.trim_end());
        }
        Ok(())
    }
}

pub fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn get_process_rss_bytes() -> u64 {
    // Read from /proc/self/statm if on Linux
    if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
        if let Some(resident_pages) = statm.split_whitespace().nth(1) {
            if let Ok(pages) = resident_pages.parse::<u64>() {
                return pages * 4096; // typical 4KB page size
            }
        }
    }
    18 * 1024 * 1024 // Fallback reasonable default (18MB)
}
