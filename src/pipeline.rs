use crate::budget::{BudgetAuditor, KB};
use crate::compress::Compressor;
use crate::config::AstroisleConfig;
use crate::integrity::Integrity;
use crate::module_graph::ModuleGraph;
use crate::service_worker::ServiceWorkerUpdater;

use std::collections::HashSet;
use std::fs;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct PipelineReport {
    pub html_pinned: usize,
    pub preloads_injected: usize,
    pub initial_js_kib: f64,
    pub initial_budget_kib: f64,
    pub precompressed_files: usize,
    pub plain_kib: f64,
    pub zstd_kib: f64,
    pub brotli_kib: f64,
    pub gzip_kib: f64,
    pub zstd_savings: f64,
    pub brotli_savings: f64,
    pub gzip_savings: f64,
    pub sw_updated: bool,
    pub sw_files: usize,
    pub sw_version: String,
    pub nstd_used: bool,
}

pub struct Pipeline;

impl Pipeline {
    pub fn run(config: &AstroisleConfig) -> Result<PipelineReport, String> {
        let dist = &config.dist;
        if !dist.exists() {
            return Err(format!("Directory does not exist: {:?}", dist));
        }

        let index_html_path = dist.join("index.html");
        if !index_html_path.exists() {
            return Err(format!("dist/index.html not found in {:?}", dist));
        }

        let mut html_content = fs::read_to_string(&index_html_path)
            .map_err(|e| format!("Failed to read index.html: {}", e))?;

        // 1. Relocate and hash boot script if enabled
        if config.integrity.hash_boot {
            let _ = Integrity::hash_and_relocate_boot_script(dist, &mut html_content, "boot.js");
        }

        // 2. Scan entry scripts and trace dependencies for modulepreload
        let mut preloads_count = 0;
        let assets_dir = dist.join("assets");
        let mut initial_scripts = HashSet::new();

        let entries = ModuleGraph::extract_entry_scripts(&html_content);
        for entry in &entries {
            initial_scripts.insert(entry.clone());
            if config.integrity.modulepreload && assets_dir.exists() {
                let deps = ModuleGraph::trace_dependencies(&assets_dir, entry);
                preloads_count += deps.len();
                for dep in &deps {
                    initial_scripts.insert(dep.clone());
                }
                html_content = Integrity::inject_module_preloads(
                    &html_content,
                    &deps,
                    dist,
                    &config.integrity.algorithm,
                );
            }
        }

        // 3. Subresource Integrity: Pin all resources
        let (pinned_html, unpinned) = Integrity::pin_html_resources(
            dist,
            &html_content,
            &config.integrity.algorithm,
        );
        if !unpinned.is_empty() {
            return Err(format!(
                "Subresource Integrity missing or unpinned on: {}",
                unpinned.join(", ")
            ));
        }

        fs::write(&index_html_path, &pinned_html)
            .map_err(|e| format!("Failed to write updated index.html: {}", e))?;

        // 4. Check for leaked source maps
        if let Err(leaks) = Integrity::check_source_map_leaks(&assets_dir) {
            return Err(format!("Source maps present in production: {}", leaks.join(", ")));
        }

        // 5. Budget audit
        let audit = BudgetAuditor::audit(
            &assets_dir,
            &initial_scripts,
            config.budgets.initial_js,
            config.budgets.lazy_chunk,
            config.budgets.css,
        )?;

        println!("{}", BudgetAuditor::format_table(&audit));

        if audit.has_failures {
            return Err("Asset budget exceeded!".to_string());
        }

        // 6. Precompression with nstd
        let compressor = Compressor::new(config.compress.nstd_bin.as_deref());
        let mut files_to_compress = Vec::new();

        let allowed_exts: HashSet<&str> = config
            .compress
            .extensions
            .iter()
            .map(|s| s.as_str())
            .collect();

        for entry in WalkDir::new(dist).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let path = entry.path();
                // Skip already compressed files
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".zst") || name.ends_with(".br") || name.ends_with(".gz") {
                        continue;
                    }
                }
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if allowed_exts.contains(ext) {
                        files_to_compress.push(path.to_path_buf());
                    }
                }
            }
        }

        let stats = compressor
            .precompress_all(&files_to_compress, config.compress.level)
            .map_err(|errs| format!("Compression failed:\n{}", errs.join("\n")))?;

        // 7. Service worker precache manifest & hash update
        let (sw_files, sw_version) = if config.sw.enable {
            ServiceWorkerUpdater::update_service_worker(dist, &config.sw.path)?
        } else {
            (0, String::new())
        };

        Ok(PipelineReport {
            html_pinned: 1,
            preloads_injected: preloads_count,
            initial_js_kib: audit.initial_js_gzip_bytes as f64 / KB,
            initial_budget_kib: config.budgets.initial_js,
            precompressed_files: stats.total_files,
            plain_kib: stats.plain_bytes as f64 / KB,
            zstd_kib: stats.zstd_bytes as f64 / KB,
            brotli_kib: stats.brotli_bytes as f64 / KB,
            gzip_kib: stats.gzip_bytes as f64 / KB,
            zstd_savings: stats.zstd_savings_pct(),
            brotli_savings: stats.brotli_savings_pct(),
            gzip_savings: stats.gzip_savings_pct(),
            sw_updated: config.sw.enable && sw_files > 0,
            sw_files,
            sw_version,
            nstd_used: compressor.nstd_location().is_some(),
        })
    }
}
