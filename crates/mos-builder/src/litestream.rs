use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LitestreamDbConfig {
    pub db_path: String,
    pub replica_type: String,
    pub bucket: String,
    pub s3_endpoint: Option<String>,
    pub replica_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LitestreamPlan {
    pub enabled: bool,
    pub detected_db_path: Option<PathBuf>,
    pub dbs: Vec<LitestreamDbConfig>,
}

pub struct LitestreamManager;

impl LitestreamManager {
    pub fn detect_sqlite(app_dir: &Path) -> Option<PathBuf> {
        // 1. Check for explicit SQLite database files
        let common_db_names = [
            "app.db",
            "data.db",
            "sqlite.db",
            "database.sqlite",
            "db.sqlite3",
        ];
        for name in common_db_names {
            let candidate = app_dir.join(name);
            if candidate.exists() {
                debug!("Detected SQLite file: {:?}", candidate);
                return Some(candidate);
            }
            let data_candidate = app_dir.join("data").join(name);
            if data_candidate.exists() {
                debug!("Detected SQLite file in data/: {:?}", data_candidate);
                return Some(data_candidate);
            }
        }

        // 2. Check for Prisma / SQLite schema files
        let prisma_schema = app_dir.join("prisma/schema.prisma");
        if prisma_schema.exists() {
            if let Ok(content) = std::fs::read_to_string(&prisma_schema) {
                if content.contains("provider = \"sqlite\"") {
                    debug!("Detected SQLite in Prisma schema");
                    return Some(app_dir.join("prisma/dev.db"));
                }
            }
        }

        // 3. Check for .env containing sqlite:///
        let env_file = app_dir.join(".env");
        if env_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&env_file) {
                for line in content.lines() {
                    if line.starts_with("DATABASE_URL=") && line.contains("sqlite") {
                        debug!("Detected SQLite DATABASE_URL in .env");
                        return Some(app_dir.join("app.db"));
                    }
                }
            }
        }

        None
    }

    pub fn generate_litestream_yaml(
        instance_id: &str,
        db_path: &Path,
        bucket: Option<&str>,
        r2_endpoint: Option<&str>,
    ) -> String {
        let bucket_name = bucket.unwrap_or("mos-replicas");
        let endpoint_str = r2_endpoint
            .map(|ep| format!("      endpoint: {}\n", ep))
            .unwrap_or_default();

        let db_target = db_path.to_str().unwrap_or("/app/app.db");

        format!(
            r#"dbs:
  - path: {}
    replicas:
      - type: s3
        bucket: {}
        path: instances/{}/{}
{}        sync-interval: 1s
"#,
            db_target,
            bucket_name,
            instance_id,
            db_path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("app.db"),
            endpoint_str
        )
    }

    pub fn generate_restore_command(config_path: &Path, db_target: &Path) -> String {
        format!(
            "litestream restore -if-replica-exists -config {} {}",
            config_path.display(),
            db_target.display()
        )
    }
}
