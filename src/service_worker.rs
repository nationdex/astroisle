use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

pub struct ServiceWorkerUpdater;

impl ServiceWorkerUpdater {
    /// Collects precache paths, computes deterministic version hash, and updates service worker placeholders.
    pub fn update_service_worker(
        dist_dir: &Path,
        sw_rel_path: &str,
    ) -> Result<(usize, String), String> {
        let sw_path = dist_dir.join(sw_rel_path);
        if !sw_path.exists() {
            return Ok((0, "none".to_string()));
        }

        let mut precache = Vec::new();

        // Standard root assets
        for root_asset in &["manifest.webmanifest", "favicon.svg", "favicon.ico"] {
            if dist_dir.join(root_asset).exists() {
                precache.push(format!("/{}", root_asset));
            }
        }

        // Icons directory
        let icons_dir = dist_dir.join("icons");
        if icons_dir.exists() {
            if let Ok(entries) = fs::read_dir(&icons_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    precache.push(format!("/icons/{}", name));
                }
            }
        }

        // Built assets in assets/
        let assets_dir = dist_dir.join("assets");
        if assets_dir.exists() {
            for entry in WalkDir::new(&assets_dir)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file() {
                    let rel = entry
                        .path()
                        .strip_prefix(dist_dir)
                        .unwrap_or(entry.path());
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    if rel_str.ends_with(".js") || rel_str.ends_with(".css") {
                        precache.push(format!("/{}", rel_str));
                    }
                }
            }
        }

        precache.sort();
        precache.dedup();

        let sw_source = fs::read_to_string(&sw_path).map_err(|e| e.to_string())?;

        // Calculate deterministic version hash
        let mut hasher = Sha256::new();
        hasher.update(sw_source.as_bytes());

        let index_path = dist_dir.join("index.html");
        if index_path.exists() {
            if let Ok(idx_bytes) = fs::read(&index_path) {
                hasher.update(&idx_bytes);
            }
        }

        for path_str in &precache {
            let file_on_disk = dist_dir.join(path_str.trim_start_matches('/'));
            if file_on_disk.exists() {
                if let Ok(content) = fs::read(&file_on_disk) {
                    hasher.update(&content);
                }
            }
        }

        let full_hash = format!("{:x}", hasher.finalize());
        let version = full_hash[..12].to_string();

        let precache_json = serde_json::to_string_pretty(&precache).map_err(|e| e.to_string())?;

        let updated_sw = sw_source
            .replace("__VERSION__", &version)
            .replace("__PRECACHE__", &precache_json);

        if updated_sw.contains("__VERSION__") || updated_sw.contains("__PRECACHE__") {
            return Err("Service worker placeholders (__VERSION__ or __PRECACHE__) were not replaced".into());
        }

        fs::write(&sw_path, updated_sw).map_err(|e| e.to_string())?;

        Ok((precache.len(), version))
    }
}
