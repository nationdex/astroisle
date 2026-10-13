use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct IslandAudit {
    pub name: String,
    pub references_in_html: usize,
    pub source_file: Option<PathBuf>,
}

pub struct IslandAuditor;

impl IslandAuditor {
    /// Discovers all micro-islands declared via `data-island="..."` in text.
    pub fn scan_html_islands(html: &str) -> HashSet<String> {
        let island_re = Regex::new(r#"data-island=["']([^"']+)["']"#).unwrap();
        let mut islands = HashSet::new();
        for caps in island_re.captures_iter(html) {
            if let Some(m) = caps.get(1) {
                islands.insert(m.as_str().to_string());
            }
        }
        islands
    }

    /// Recursively scans directory or file for data-island references.
    pub fn scan_path_islands(path: &Path) -> (HashSet<String>, HashSet<String>) {
        let mut islands = HashSet::new();
        let mut wcs = HashSet::new();

        if path.is_file() {
            if let Ok(content) = fs::read_to_string(path) {
                islands.extend(Self::scan_html_islands(&content));
                wcs.extend(Self::scan_web_components(&content));
            }
        } else if path.is_dir() {
            for entry in walkdir::WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    let p = entry.path();
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if ["html", "rs", "astro", "jsx", "tsx", "vue", "svelte"].contains(&ext) {
                        if let Ok(content) = fs::read_to_string(p) {
                            islands.extend(Self::scan_html_islands(&content));
                            wcs.extend(Self::scan_web_components(&content));
                        }
                    }
                }
            }
        }

        (islands, wcs)
    }

    /// Discovers all custom element Web Components (<my-component ...>) in HTML.
    pub fn scan_web_components(html: &str) -> HashSet<String> {
        let custom_re = Regex::new(r#"<([a-z][a-z0-9]*-[a-z0-9-]+)\b"#).unwrap();
        let mut components = HashSet::new();
        for caps in custom_re.captures_iter(html) {
            if let Some(m) = caps.get(1) {
                components.insert(m.as_str().to_string());
            }
        }
        components
    }

    /// Scans a source directory for island implementations (e.g. `src/islands/*.ts` or `src/islands/*.js`).
    pub fn find_island_sources(islands_dir: &Path) -> Vec<(String, PathBuf)> {
        let mut list = Vec::new();
        if !islands_dir.exists() {
            return list;
        }

        if let Ok(entries) = fs::read_dir(islands_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    if stem.ends_with(".test") || stem == "lazy" {
                        continue;
                    }
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext == "ts" || ext == "js" {
                            list.push((stem.to_string(), path));
                        }
                    }
                }
            }
        }

        list
    }

    /// Emits the zero-allocation, ultra-compact Alpine / Zero-JS micro-islands runtime script.
    pub fn generate_alpine_runtime(islands_map: &[(String, String)]) -> String {
        let mut map_entries = String::new();
        for (name, path) in islands_map {
            map_entries.push_str(&format!(r#"  "{}": () => import("{}"),\n"#, name, path));
        }

        format!(
            r#"// AstroIsle: Micro-Islands Loader Runtime for Alpine / Zero-JS
export function initAstroIslands(Alpine, manifest) {{
  const registry = manifest || {{
{}  }};
  const ready = new Set();
  const selector = "[data-island][x-ignore]";

  async function activate(root) {{
    const nodes = (root || document).querySelectorAll(selector);
    for (const el of nodes) {{
      const name = el.dataset.island;
      const loader = registry[name];
      if (!loader) continue;
      try {{
        if (!ready.has(name)) {{
          const mod = await loader();
          if (mod && mod.register) mod.register(Alpine);
          ready.add(name);
        }}
      }} catch (err) {{
        console.warn("[astroisle] failed loading island", name, err);
        continue;
      }}
      if (!el.isConnected || !el.hasAttribute("x-ignore")) continue;
      el.removeAttribute("x-ignore");
      Reflect.deleteProperty(el, "_x_ignore");
      el.setAttribute("x-data", name);
      if (Alpine.initTree) Alpine.initTree(el);
    }}
  }}

  const runLater = () => void activate(document);
  if ("requestIdleCallback" in window) {{
    requestIdleCallback(runLater, {{ timeout: 2500 }});
  }} else {{
    setTimeout(runLater, 300);
  }}

  if (typeof document !== "undefined") {{
    document.body.addEventListener("htmx:afterSettle", () => void activate(document));
  }}

  return {{ activate }};
}}
"#,
            map_entries
        )
    }
}
