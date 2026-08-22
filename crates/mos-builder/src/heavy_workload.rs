use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeavyWorkloadManifest {
    pub name: String,
    pub typst_binary: Option<PathBuf>,
    pub rhwp_binary: Option<PathBuf>,
    pub font_directories: Vec<PathBuf>,
    pub total_asset_bytes: u64,
    pub outbound_api_allowed: Vec<String>,
}

pub struct HeavyWorkloadDetector;

impl HeavyWorkloadDetector {
    /// Detects if an application needs heavy native typesetting or font assets
    pub fn inspect_app_needs(app_dir: &Path) -> Option<HeavyWorkloadManifest> {
        let is_heavy = app_dir.join("Cargo.toml").exists()
            && std::fs::read_to_string(app_dir.join("Cargo.toml"))
                .map(|c| {
                    c.contains("typst")
                        || c.contains("rhwp")
                        || c.contains("typeset")
                        || c.contains("typesetting")
                })
                .unwrap_or(false)
            || app_dir.join("mos.toml").exists()
                && std::fs::read_to_string(app_dir.join("mos.toml"))
                    .map(|c| c.contains("typst") || c.contains("heavy") || c.contains("fonts"))
                    .unwrap_or(false);

        if !is_heavy {
            return None;
        }

        let app_name = app_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("heavy-workload-app")
            .to_string();

        let mut font_dirs = Vec::new();
        let mut total_bytes = 0;

        let home_dir = std::env::var("HOME").map(PathBuf::from).ok();

        // Check standard system and user font paths
        let candidate_font_paths = vec![
            PathBuf::from("/usr/share/fonts/opentype/noto"),
            PathBuf::from("/usr/share/fonts/truetype/noto"),
            PathBuf::from("/usr/share/fonts/noto"),
            PathBuf::from("/usr/share/fonts"),
            PathBuf::from("/usr/local/share/fonts"),
            home_dir
                .as_ref()
                .map(|h| h.join(".local/share/fonts"))
                .unwrap_or_default(),
            home_dir
                .as_ref()
                .map(|h| h.join(".fonts"))
                .unwrap_or_default(),
        ];

        for font_path in candidate_font_paths {
            if font_path.exists() && font_path.is_dir() {
                font_dirs.push(font_path.clone());
                if let Ok(entries) = std::fs::read_dir(&font_path) {
                    for entry in entries.flatten() {
                        if let Ok(meta) = entry.metadata() {
                            if meta.is_file() {
                                total_bytes += meta.len();
                            }
                        }
                    }
                }
                break; // Use the most specific font root found
            }
        }

        // Search for Typst and rhwp binary across standard PATHs
        let mut bin_candidates = vec![
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
        ];

        if let Some(ref home) = home_dir {
            bin_candidates.push(home.join(".cargo/bin"));
            bin_candidates.push(home.join(".local/bin"));
            bin_candidates.push(home.join(".linuxbrew/bin"));
        }

        let typst_bin = bin_candidates
            .iter()
            .map(|dir| dir.join("typst"))
            .find(|p| p.exists());

        let rhwp_bin = bin_candidates
            .iter()
            .map(|dir| dir.join("rhwp"))
            .find(|p| p.exists());

        Some(HeavyWorkloadManifest {
            name: app_name,
            typst_binary: typst_bin,
            rhwp_binary: rhwp_bin,
            font_directories: font_dirs,
            total_asset_bytes: total_bytes,
            outbound_api_allowed: vec![
                "generativelanguage.googleapis.com".to_string(),
                "api.anthropic.com".to_string(),
                "api.openai.com".to_string(),
            ],
        })
    }

    /// Bundles heavy assets into target rootfs mount tree
    pub fn bundle_assets(manifest: &HeavyWorkloadManifest, target_rootfs_dir: &Path) -> Result<()> {
        info!(
            "Bundling heavy workload assets for '{}' (Total asset size: {} bytes)",
            manifest.name, manifest.total_asset_bytes
        );

        let fonts_target = target_rootfs_dir.join("usr/share/fonts");
        std::fs::create_dir_all(&fonts_target)?;

        for font_dir in &manifest.font_directories {
            if font_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(font_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            let dest = fonts_target.join(entry.file_name());
                            let _ = std::fs::copy(&path, &dest);
                        }
                    }
                }
            }
        }

        let bin_target = target_rootfs_dir.join("usr/local/bin");
        std::fs::create_dir_all(&bin_target)?;

        if let Some(ref typst) = manifest.typst_binary {
            let dest = bin_target.join("typst");
            let _ = std::fs::copy(typst, &dest);
        }

        if let Some(ref rhwp) = manifest.rhwp_binary {
            let dest = bin_target.join("rhwp");
            let _ = std::fs::copy(rhwp, &dest);
        }

        Ok(())
    }
}
