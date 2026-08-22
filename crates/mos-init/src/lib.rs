pub mod mount;
pub mod network;
pub mod supervisor;
pub mod vsock_ipc;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitConfig {
    pub app_cmd: String,
    pub app_args: Vec<String>,
    pub env_vars: HashMap<String, String>,
    pub guest_ip: Option<String>,
    pub gateway_ip: Option<String>,
    pub enable_litestream: bool,
    pub litestream_config_path: Option<PathBuf>,
}

impl Default for InitConfig {
    fn default() -> Self {
        Self {
            app_cmd: "/bin/sh".to_string(),
            app_args: Vec::new(),
            env_vars: HashMap::new(),
            guest_ip: Some("172.16.0.2".to_string()),
            gateway_ip: Some("172.16.0.1".to_string()),
            enable_litestream: false,
            litestream_config_path: None,
        }
    }
}
