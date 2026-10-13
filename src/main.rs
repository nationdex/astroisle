use astroisle::budget::BudgetAuditor;
use astroisle::compress::Compressor;
use astroisle::config::AstroisleConfig;
use astroisle::integrity::Integrity;
use astroisle::islands::IslandAuditor;
use astroisle::module_graph::ModuleGraph;
use astroisle::pipeline::Pipeline;
use clap::{Parser, Subcommand};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::exit;

#[derive(Parser, Debug)]
#[command(
    name = "astroisle",
    version,
    about = "Micro-Islands Bundler & Asset Delivery Pipeline for Alpine / Zero-JS"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to distribution directory (defaults to "dist")
    #[arg(short, long, global = true, default_value = "dist")]
    dist: PathBuf,

    /// Initial JavaScript budget in KiB
    #[arg(long, global = true)]
    budget_initial: Option<f64>,

    /// Lazy chunk budget in KiB
    #[arg(long, global = true)]
    budget_lazy: Option<f64>,

    /// CSS stylesheet budget in KiB
    #[arg(long, global = true)]
    budget_css: Option<f64>,

    /// Explicit path to nstd binary
    #[arg(long, global = true)]
    nstd_bin: Option<PathBuf>,

    /// Compression level for nstd (1..19)
    #[arg(short, long, global = true, default_value = "9")]
    level: i32,

    /// Disable service worker precache injection
    #[arg(long, global = true)]
    no_sw: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the complete automated pipeline (audit, modulepreload, SRI, precompression, SW)
    Run,

    /// Audit bundle sizes and verify budgets
    Audit,

    /// Inject Subresource Integrity (SRI) and modulepreload into HTML
    Integrity,

    /// Precompress assets with nstd (.zst, .br, .gz) with hash-verification
    Compress,

    /// Audit micro-islands and Web Components in HTML or template directory
    Islands {
        /// Optional path to HTML file or templates directory (defaults to dist/index.html)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Emit the Alpine / Zero-JS micro-islands loader runtime
    Runtime,
}

fn apply_cli_overrides(cfg: &mut AstroisleConfig, cli: &Cli) {
    cfg.dist = cli.dist.clone();
    if let Some(b) = cli.budget_initial {
        cfg.budgets.initial_js = b;
    }
    if let Some(b) = cli.budget_lazy {
        cfg.budgets.lazy_chunk = b;
    }
    if let Some(b) = cli.budget_css {
        cfg.budgets.css = b;
    }
    if let Some(ref p) = cli.nstd_bin {
        cfg.compress.nstd_bin = Some(p.clone());
    }
    cfg.compress.level = cli.level;
    if cli.no_sw {
        cfg.sw.enable = false;
    }
}

fn main() {
    let cli = Cli::parse();
    let mut config = AstroisleConfig::load_from_dir(&cli.dist);
    apply_cli_overrides(&mut config, &cli);

    let command = cli.command.unwrap_or(Commands::Run);

    match command {
        Commands::Run => {
            println!("🏝️  AstroIsle Pipeline running on {:?}", config.dist);
            match Pipeline::run(&config) {
                Ok(report) => {
                    println!("\n✅ Pipeline complete:");
                    println!(
                        "   Module preloads injected: {}",
                        report.preloads_injected
                    );
                    println!(
                        "   Initial JS size: {:.1} KiB (budget: {:.1} KiB)",
                        report.initial_js_kib, report.initial_budget_kib
                    );
                    println!(
                        "   Precompressed {} assets (nstd: {}):",
                        report.precompressed_files,
                        if report.nstd_used { "active" } else { "native fallback" }
                    );
                    println!(
                        "     • Plain:  {:.1} KiB",
                        report.plain_kib
                    );
                    println!(
                        "     • Zstd:   {:.1} KiB (-{:.1}%)",
                        report.zstd_kib, report.zstd_savings
                    );
                    println!(
                        "     • Brotli: {:.1} KiB (-{:.1}%)",
                        report.brotli_kib, report.brotli_savings
                    );
                    println!(
                        "     • Gzip:   {:.1} KiB (-{:.1}%)",
                        report.gzip_kib, report.gzip_savings
                    );
                    if report.sw_updated {
                        println!(
                            "   Service worker: precached {} files (v{})",
                            report.sw_files, report.sw_version
                        );
                    }
                }
                Err(e) => {
                    eprintln!("\n❌ Pipeline failed: {}", e);
                    exit(1);
                }
            }
        }
        Commands::Audit => {
            let index_html_path = config.dist.join("index.html");
            let mut initial_scripts = HashSet::new();

            if index_html_path.exists() {
                if let Ok(html) = fs::read_to_string(&index_html_path) {
                    let entries = ModuleGraph::extract_entry_scripts(&html);
                    for entry in entries {
                        initial_scripts.insert(entry.clone());
                        let deps = ModuleGraph::trace_dependencies(&config.dist.join("assets"), &entry);
                        for d in deps {
                            initial_scripts.insert(d);
                        }
                    }
                }
            }

            match BudgetAuditor::audit(
                &config.dist.join("assets"),
                &initial_scripts,
                config.budgets.initial_js,
                config.budgets.lazy_chunk,
                config.budgets.css,
            ) {
                Ok(report) => {
                    println!("{}", BudgetAuditor::format_table(&report));
                    if report.has_failures {
                        eprintln!("\n❌ Budget audit failed!");
                        exit(1);
                    } else {
                        println!("\n✅ Budget audit passed!");
                    }
                }
                Err(e) => {
                    eprintln!("Audit error: {}", e);
                    exit(1);
                }
            }
        }
        Commands::Integrity => {
            let index_html_path = config.dist.join("index.html");
            if !index_html_path.exists() {
                eprintln!("Missing {:?}", index_html_path);
                exit(1);
            }
            let mut html = fs::read_to_string(&index_html_path).expect("read index.html");
            let entries = ModuleGraph::extract_entry_scripts(&html);
            let assets_dir = config.dist.join("assets");
            for entry in entries {
                let deps = ModuleGraph::trace_dependencies(&assets_dir, &entry);
                html = Integrity::inject_module_preloads(&html, &deps, &config.dist, &config.integrity.algorithm);
            }
            let (pinned, unpinned) = Integrity::pin_html_resources(&config.dist, &html, &config.integrity.algorithm);
            if !unpinned.is_empty() {
                eprintln!("Unpinned: {}", unpinned.join(", "));
                exit(1);
            }
            fs::write(&index_html_path, pinned).expect("write index.html");
            println!("✅ Resources pinned with Subresource Integrity!");
        }
        Commands::Compress => {
            let compressor = Compressor::new(config.compress.nstd_bin.as_deref());
            let mut files = Vec::new();
            for entry in walkdir::WalkDir::new(&config.dist).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    let path = entry.path();
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ["js", "css", "svg", "json", "html", "webmanifest"].contains(&ext) {
                            files.push(path.to_path_buf());
                        }
                    }
                }
            }
            match compressor.precompress_all(&files, config.compress.level) {
                Ok(stats) => {
                    println!(
                        "✅ Precompressed {} files with nstd (Zstd -{:.1}%, Brotli -{:.1}%, Gzip -{:.1}%)",
                        stats.total_files,
                        stats.zstd_savings_pct(),
                        stats.brotli_savings_pct(),
                        stats.gzip_savings_pct()
                    );
                }
                Err(errs) => {
                    eprintln!("Compression errors:\n{}", errs.join("\n"));
                    exit(1);
                }
            }
        }
        Commands::Islands { path } => {
            let target_path = path.unwrap_or_else(|| config.dist.join("index.html"));
            if !target_path.exists() {
                eprintln!("Target path not found: {:?}", target_path);
                exit(1);
            }
            let (islands, web_components) = IslandAuditor::scan_path_islands(&target_path);

            println!("🏝️  Discovered Micro-Islands (data-island):");
            if islands.is_empty() {
                println!("    (none)");
            } else {
                let mut sorted: Vec<_> = islands.into_iter().collect();
                sorted.sort();
                for isl in sorted {
                    println!("    • {}", isl);
                }
            }

            println!("\n🧩 Discovered Web Components (<tag-name>):");
            if web_components.is_empty() {
                println!("    (none)");
            } else {
                let mut sorted: Vec<_> = web_components.into_iter().collect();
                sorted.sort();
                for wc in sorted {
                    println!("    • {}", wc);
                }
            }
        }
        Commands::Runtime => {
            let runtime_code = IslandAuditor::generate_alpine_runtime(&[]);
            println!("{}", runtime_code);
        }
    }
}
