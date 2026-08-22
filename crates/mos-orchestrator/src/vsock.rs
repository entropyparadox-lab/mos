use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixStream;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsockDeviceConfig {
    pub vsock_id: String,
    pub guest_cid: u32,
    pub uds_path: PathBuf,
}

impl VsockDeviceConfig {
    pub fn new(guest_cid: u32, uds_path: impl Into<PathBuf>) -> Self {
        Self {
            vsock_id: "vsock0".to_string(),
            guest_cid,
            uds_path: uds_path.into(),
        }
    }
}

pub struct VsockHostChannel {
    pub socket_path: PathBuf,
}

impl VsockHostChannel {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    pub async fn read_line_stream<F>(&self, mut on_line: F) -> Result<()>
    where
        F: FnMut(String) + Send + 'static,
    {
        if !self.socket_path.exists() {
            debug!("Vsock path {:?} does not exist yet", self.socket_path);
            return Ok(());
        }

        let stream = UnixStream::connect(&self.socket_path).await?;
        let mut reader = BufReader::new(stream).lines();

        while let Ok(Some(line)) = reader.next_line().await {
            on_line(line);
        }

        Ok(())
    }
}
