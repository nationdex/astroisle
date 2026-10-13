use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_INITIAL_JS_BUDGET_KB: f64 = 104.0;
pub const DEFAULT_LAZY_CHUNK_BUDGET_KB: f64 = 70.0;
pub const DEFAULT_CSS_BUDGET_KB: f64 = 14.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    #[serde(default = "default_initial_js")]
    pub initial_js: f64,
    #[serde(default = "default_lazy_chunk")]
    pub lazy_chunk: f64,
    #[serde(default = "default_css")]
    pub css: f64,
}

fn default_initial_js() -> f64 {
    DEFAULT_INITIAL_JS_BUDGET_KB
}
fn default_lazy_chunk() -> f64 {
    DEFAULT_LAZY_CHUNK_BUDGET_KB
}
fn default_css() -> f64 {
    DEFAULT_CSS_BUDGET_KB
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            initial_js: DEFAULT_INITIAL_JS_BUDGET_KB,
            lazy_chunk: DEFAULT_LAZY_CHUNK_BUDGET_KB,
            css: DEFAULT_CSS_BUDGET_KB,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressConfig {
    #[serde(default = "default_compress_extensions")]
    pub extensions: Vec<String>,
    #[serde(default = "default_compress_level")]
    pub level: i32,
    #[serde(default)]
    pub nstd_bin: Option<PathBuf>,
}

fn default_compress_extensions() -> Vec<String> {
    vec![
        "js".into(),
        "css".into(),
        "svg".into(),
        "json".into(),
        "html".into(),
        "webmanifest".into(),
    ]
}

fn default_compress_level() -> i32 {
    9
}

impl Default for CompressConfig {
    fn default() -> Self {
        Self {
            extensions: default_compress_extensions(),
            level: default_compress_level(),
            nstd_bin: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityConfig {
    #[serde(default = "default_integrity_algo")]
    pub algorithm: String,
    #[serde(default = "default_true")]
    pub modulepreload: bool,
    #[serde(default = "default_true")]
    pub hash_boot: bool,
}

fn default_integrity_algo() -> String {
    "sha384".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for IntegrityConfig {
    fn default() -> Self {
        Self {
            algorithm: default_integrity_algo(),
            modulepreload: true,
            hash_boot: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceWorkerConfig {
    #[serde(default = "default_sw_path")]
    pub path: String,
    #[serde(default = "default_true")]
    pub enable: bool,
}

fn default_sw_path() -> String {
    "sw.js".to_string()
}

impl Default for ServiceWorkerConfig {
    fn default() -> Self {
        Self {
            path: default_sw_path(),
            enable: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstroisleConfig {
    #[serde(default = "default_dist_dir")]
    pub dist: PathBuf,
    #[serde(default)]
    pub budgets: BudgetConfig,
    #[serde(default)]
    pub compress: CompressConfig,
    #[serde(default)]
    pub integrity: IntegrityConfig,
    #[serde(default)]
    pub sw: ServiceWorkerConfig,
}

fn default_dist_dir() -> PathBuf {
    PathBuf::from("dist")
}

impl Default for AstroisleConfig {
    fn default() -> Self {
        Self {
            dist: default_dist_dir(),
            budgets: BudgetConfig::default(),
            compress: CompressConfig::default(),
            integrity: IntegrityConfig::default(),
            sw: ServiceWorkerConfig::default(),
        }
    }
}

impl AstroisleConfig {
    pub fn load_from_dir(dir: &Path) -> Self {
        // Look for astroisle.json or astroisle.toml in dir or dir.parent()
        let candidates = [
            dir.join("astroisle.json"),
            dir.join("astroisle.toml"),
            dir.join("../astroisle.json"),
        ];

        for path in candidates {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if path.extension().is_some_and(|ext| ext == "json") {
                        if let Ok(cfg) = serde_json::from_str::<AstroisleConfig>(&content) {
                            return cfg;
                        }
                    }
                }
            }
        }

        Self::default()
    }
}
