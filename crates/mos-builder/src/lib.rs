pub mod heavy_workload;
pub mod litestream;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tracing::info;

pub use heavy_workload::{HeavyWorkloadDetector, HeavyWorkloadManifest};
pub use litestream::{LitestreamDbConfig, LitestreamManager, LitestreamPlan};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanResult {
    pub provider: String,
    pub build_cmds: Vec<String>,
    pub start_cmd: Option<String>,
    pub variables: Option<serde_json::Value>,
    pub raw_plan: serde_json::Value,
    pub litestream_plan: Option<LitestreamPlan>,
}

pub struct BuilderEngine {
    nixpacks_bin: PathBuf,
}

impl BuilderEngine {
    pub fn new(nixpacks_bin: PathBuf) -> Self {
        Self { nixpacks_bin }
    }

    pub async fn plan(&self, app_dir: &Path) -> Result<PlanResult> {
        info!(app_dir = %app_dir.display(), "Generating build plan with Nixpacks...");

        if self.nixpacks_bin.exists() {
            let output = Command::new(&self.nixpacks_bin)
                .arg("plan")
                .arg(app_dir)
                .arg("--format")
                .arg("json")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
                .context("Failed to execute nixpacks plan command")?;

            if !output.status.success() {
                let err_msg = String::from_utf8_lossy(&output.stderr);
                bail!("Nixpacks plan failed: {}", err_msg);
            }

            let json_val: serde_json::Value = serde_json::from_slice(&output.stdout)
                .context("Failed to parse nixpacks JSON output")?;

            // 1. Detect provider (from variables.NIXPACKS_METADATA or providers array)
            let provider = json_val
                .get("variables")
                .and_then(|v| v.get("NIXPACKS_METADATA"))
                .and_then(|m| m.as_str())
                .map(String::from)
                .or_else(|| {
                    json_val
                        .get("providers")
                        .and_then(|p| p.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|v| v.as_str())
                        .map(String::from)
                })
                .unwrap_or_else(|| "generic".to_string());

            // 2. Detect start command (start.cmd or phases.start.cmds)
            let start_cmd = json_val
                .get("start")
                .and_then(|s| s.get("cmd"))
                .and_then(|c| c.as_str())
                .map(String::from)
                .or_else(|| {
                    json_val
                        .get("phases")
                        .and_then(|p| p.get("start"))
                        .and_then(|s| s.get("cmds"))
                        .and_then(|c| c.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|v| v.as_str())
                        .map(String::from)
                });

            // 3. Detect build commands
            let build_cmds = json_val
                .get("phases")
                .and_then(|p| p.get("build"))
                .and_then(|b| b.get("cmds"))
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            // 4. Auto-detect SQLite & Litestream requirements
            let detected_db = LitestreamManager::detect_sqlite(app_dir);
            let litestream_plan = detected_db.as_ref().map(|db_path| LitestreamPlan {
                enabled: true,
                detected_db_path: Some(db_path.clone()),
                dbs: vec![LitestreamDbConfig {
                    db_path: db_path.to_string_lossy().to_string(),
                    replica_type: "s3".to_string(),
                    bucket: "mos-replicas".to_string(),
                    s3_endpoint: None,
                    replica_path: format!(
                        "instances/default/{}",
                        db_path.file_name().unwrap().to_string_lossy()
                    ),
                }],
            });

            return Ok(PlanResult {
                provider,
                build_cmds,
                start_cmd,
                variables: json_val.get("variables").cloned(),
                raw_plan: json_val,
                litestream_plan,
            });
        }

        // Fallback heuristic planning when nixpacks CLI binary is not found
        let (provider, build_cmds, start_cmd) = if app_dir.join("Cargo.toml").exists() {
            (
                "rust".to_string(),
                vec!["cargo build --release".to_string()],
                Some("./target/release/app".to_string()),
            )
        } else if app_dir.join("package.json").exists() {
            (
                "node".to_string(),
                vec!["npm install".to_string(), "npm run build".to_string()],
                Some("npm run start".to_string()),
            )
        } else if app_dir.join("requirements.txt").exists()
            || app_dir.join("pyproject.toml").exists()
        {
            (
                "python".to_string(),
                vec!["pip install -r requirements.txt".to_string()],
                Some("uvicorn main:app --host 0.0.0.0 --port 8080".to_string()),
            )
        } else {
            ("generic".to_string(), vec![], None)
        };

        let detected_db = LitestreamManager::detect_sqlite(app_dir);
        let litestream_plan = detected_db.as_ref().map(|db_path| LitestreamPlan {
            enabled: true,
            detected_db_path: Some(db_path.clone()),
            dbs: vec![LitestreamDbConfig {
                db_path: db_path.to_string_lossy().to_string(),
                replica_type: "s3".to_string(),
                bucket: "mos-replicas".to_string(),
                s3_endpoint: None,
                replica_path: format!(
                    "instances/default/{}",
                    db_path.file_name().unwrap().to_string_lossy()
                ),
            }],
        });

        Ok(PlanResult {
            provider,
            build_cmds,
            start_cmd,
            variables: None,
            raw_plan: serde_json::json!({ "fallback": true }),
            litestream_plan,
        })
    }

    pub async fn prepare_instance_disk(
        &self,
        base_rootfs: &Path,
        dest_rootfs: &Path,
    ) -> Result<()> {
        if let Some(parent) = dest_rootfs.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(base_rootfs, dest_rootfs).await?;
        Ok(())
    }
}
