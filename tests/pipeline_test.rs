use astroisle::config::AstroisleConfig;
use astroisle::islands::IslandAuditor;
use astroisle::pipeline::Pipeline;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_full_pipeline_success() {
    let dir = tempdir().unwrap();
    let dist = dir.path();
    let assets = dist.join("assets");
    fs::create_dir_all(&assets).unwrap();

    // Create entry script that imports chunk1
    fs::write(
        assets.join("entry.js"),
        r#"import { modal } from "./chunk1.js"; console.log("entry");"#,
    )
    .unwrap();
    fs::write(assets.join("chunk1.js"), r#"export const modal = 42;"#).unwrap();
    fs::write(assets.join("style.css"), "body { background: #000; }").unwrap();

    // Boot script in root
    fs::write(dist.join("boot.js"), "console.log('theme boot');").unwrap();

    // sw.js in root
    fs::write(
        dist.join("sw.js"),
        r#"const V = "__VERSION__"; const P = __PRECACHE__;"#,
    )
    .unwrap();

    // index.html
    let html = r#"<!DOCTYPE html>
<html>
<head>
  <script src="/boot.js"></script>
  <link rel="stylesheet" href="/assets/style.css">
  <script type="module" src="/assets/entry.js"></script>
</head>
<body data-island="modal">
  <div data-island="ad" x-ignore></div>
  <app-chart></app-chart>
</body>
</html>"#;
    fs::write(dist.join("index.html"), html).unwrap();

    let mut config = AstroisleConfig::default();
    config.dist = dist.to_path_buf();
    config.budgets.initial_js = 104.0;
    config.budgets.lazy_chunk = 70.0;
    config.budgets.css = 14.0;

    let report = Pipeline::run(&config).expect("Pipeline should succeed");

    assert_eq!(report.html_pinned, 1);
    assert!(report.preloads_injected >= 1);
    assert!(report.precompressed_files > 0);
    assert!(report.sw_updated);
    assert!(!report.sw_version.is_empty());

    // Check index.html modifications
    let updated_html = fs::read_to_string(dist.join("index.html")).unwrap();
    assert!(updated_html.contains(r#"<link rel="modulepreload""#));
    assert!(updated_html.contains(r#"integrity="sha384-"#));
    assert!(!updated_html.contains(r#"src="/boot.js""#)); // replaced with hashed
    assert!(updated_html.contains("boot-"));

    // Check precompressed files exist
    assert!(assets.join("entry.js.zst").exists());
    assert!(assets.join("entry.js.br").exists());
    assert!(assets.join("entry.js.gz").exists());

    // Check sw.js replaced placeholders
    let updated_sw = fs::read_to_string(dist.join("sw.js")).unwrap();
    assert!(!updated_sw.contains("__VERSION__"));
    assert!(!updated_sw.contains("__PRECACHE__"));
    assert!(updated_sw.contains(&report.sw_version));

    // Check island discovery
    let islands = IslandAuditor::scan_html_islands(&updated_html);
    assert!(islands.contains("modal"));
    assert!(islands.contains("ad"));

    let wcs = IslandAuditor::scan_web_components(&updated_html);
    assert!(wcs.contains("app-chart"));
}

#[test]
fn test_budget_exceeded_fails() {
    let dir = tempdir().unwrap();
    let dist = dir.path();
    let assets = dist.join("assets");
    fs::create_dir_all(&assets).unwrap();

    // Create a file with high entropy/unique code that doesn't compress to nothing
    let mut large_code = String::new();
    for i in 0..1000 {
        large_code.push_str(&format!("export const var_{}_{} = '{}';\n", i, i * 7919, "a".repeat(20)));
    }
    fs::write(assets.join("entry.js"), large_code).unwrap();

    fs::write(
        dist.join("index.html"),
        r#"<!DOCTYPE html><html><head><script type="module" src="/assets/entry.js"></script></head><body></body></html>"#,
    )
    .unwrap();

    let mut config = AstroisleConfig::default();
    config.dist = dist.to_path_buf();
    config.budgets.initial_js = 0.5; // 0.5 KiB budget -> will fail!

    let result = Pipeline::run(&config);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("budget"));
}

#[test]
fn test_source_map_leak_fails() {
    let dir = tempdir().unwrap();
    let dist = dir.path();
    let assets = dist.join("assets");
    fs::create_dir_all(&assets).unwrap();

    fs::write(assets.join("entry.js"), "console.log(1);").unwrap();
    fs::write(assets.join("entry.js.map"), "{}").unwrap(); // Leaked .map file!

    fs::write(
        dist.join("index.html"),
        r#"<!DOCTYPE html><html><head><script type="module" src="/assets/entry.js"></script></head><body></body></html>"#,
    )
    .unwrap();

    let mut config = AstroisleConfig::default();
    config.dist = dist.to_path_buf();

    let result = Pipeline::run(&config);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Source maps present"));
}
