use flate2::write::GzEncoder;
use flate2::Compression;
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const KB: f64 = 1024.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetKind {
    InitialJs,
    LazyIsland,
    Css,
    Other,
}

#[derive(Debug, Clone)]
pub struct AssetAuditRow {
    pub filename: String,
    pub path: PathBuf,
    pub kind: AssetKind,
    pub raw_bytes: u64,
    pub gzip_bytes: u64,
    pub over_budget: bool,
    pub budget_limit_kb: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct AuditReport {
    pub rows: Vec<AssetAuditRow>,
    pub initial_js_gzip_bytes: u64,
    pub initial_js_budget_kb: f64,
    pub initial_js_over_budget: bool,
    pub has_failures: bool,
}

pub struct BudgetAuditor;

impl BudgetAuditor {
    /// Measure gzip compressed size in memory.
    pub fn measure_gzip(bytes: &[u8]) -> u64 {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        let _ = encoder.write_all(bytes);
        encoder.finish().map(|v| v.len() as u64).unwrap_or(0)
    }

    /// Run full audit on `assets_dir` with knowledge of `initial_scripts` detected in `index.html`.
    pub fn audit(
        assets_dir: &Path,
        initial_scripts: &HashSet<String>,
        initial_budget_kb: f64,
        lazy_budget_kb: f64,
        css_budget_kb: f64,
    ) -> Result<AuditReport, String> {
        if !assets_dir.exists() {
            return Err(format!("Assets directory not found: {:?}", assets_dir));
        }

        let mut rows = Vec::new();
        let entries = fs::read_dir(assets_dir).map_err(|e| e.to_string())?;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let filename = path.file_name().unwrap().to_string_lossy().to_string();
            let is_js = filename.ends_with(".js");
            let is_css = filename.ends_with(".css");

            if !is_js && !is_css {
                continue;
            }

            let bytes = fs::read(&path).map_err(|e| e.to_string())?;
            let raw_bytes = bytes.len() as u64;
            let gzip_bytes = Self::measure_gzip(&bytes);

            let (kind, budget_limit) = if is_css {
                (AssetKind::Css, Some(css_budget_kb))
            } else if initial_scripts.contains(&filename) {
                (AssetKind::InitialJs, None)
            } else {
                (AssetKind::LazyIsland, Some(lazy_budget_kb))
            };

            let over_budget = match (kind.clone(), budget_limit) {
                (AssetKind::LazyIsland, Some(limit)) => gzip_bytes as f64 > limit * KB,
                (AssetKind::Css, Some(limit)) => gzip_bytes as f64 > limit * KB,
                _ => false,
            };

            rows.push(AssetAuditRow {
                filename,
                path,
                kind,
                raw_bytes,
                gzip_bytes,
                over_budget,
                budget_limit_kb: budget_limit,
            });
        }

        // Sort descending by gzip size
        rows.sort_by_key(|a| std::cmp::Reverse(a.gzip_bytes));

        let initial_js_gzip_bytes: u64 = rows
            .iter()
            .filter(|r| r.kind == AssetKind::InitialJs)
            .map(|r| r.gzip_bytes)
            .sum();

        let initial_js_over_budget = initial_js_gzip_bytes as f64 > initial_budget_kb * KB;
        let any_chunk_over = rows.iter().any(|r| r.over_budget);
        let has_failures = initial_js_over_budget || any_chunk_over;

        Ok(AuditReport {
            rows,
            initial_js_gzip_bytes,
            initial_js_budget_kb: initial_budget_kb,
            initial_js_over_budget,
            has_failures,
        })
    }

    /// Formats audit results into clean formatted output.
    pub fn format_table(report: &AuditReport) -> String {
        let mut out = String::new();
        out.push_str("Bundle audit (gzip):\n");

        for r in &report.rows {
            let kind_str = match r.kind {
                AssetKind::InitialJs => "initial",
                AssetKind::LazyIsland => "lazy",
                AssetKind::Css => "css",
                AssetKind::Other => "other",
            };

            let size_kb = r.gzip_bytes as f64 / KB;
            let flag = if r.over_budget {
                format!(
                    " [OVER BUDGET: limit {:.1} KiB]",
                    r.budget_limit_kb.unwrap_or(0.0)
                )
            } else {
                String::new()
            };

            out.push_str(&format!(
                "  {:8} {:>7.1} KiB  {}{}\n",
                kind_str, size_kb, r.filename, flag
            ));
        }

        let init_kb = report.initial_js_gzip_bytes as f64 / KB;
        out.push_str(&format!(
            "\nInitial JS: {:.1} KiB of {:.1} KiB budget{}",
            init_kb,
            report.initial_js_budget_kb,
            if report.initial_js_over_budget {
                " (OVER BUDGET)"
            } else {
                " (PASS)"
            }
        ));

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_audit_budget() {
        let dir = tempdir().unwrap();
        let assets = dir.path().join("assets");
        fs::create_dir_all(&assets).unwrap();

        fs::write(assets.join("entry.js"), b"console.log('entry');").unwrap();
        fs::write(assets.join("modal.js"), b"console.log('modal island');").unwrap();
        fs::write(assets.join("app.css"), b"body { margin: 0; }").unwrap();

        let mut initial = HashSet::new();
        initial.insert("entry.js".to_string());

        let report = BudgetAuditor::audit(&assets, &initial, 104.0, 70.0, 14.0).unwrap();
        assert!(!report.has_failures);
        assert_eq!(report.rows.len(), 3);
    }
}
