use regex::Regex;
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::Path;

/// Discovers module dependencies imported by JavaScript files.
pub struct ModuleGraph;

impl ModuleGraph {
    /// Recursively walks static imports in JavaScript files inside `assets_dir`
    /// starting from `entry_file` (e.g. "index.xxxx.js"), and returns all discovered chunk filenames in order.
    pub fn trace_dependencies(assets_dir: &Path, entry_file: &str) -> Vec<String> {
        let mut visited = HashSet::new();
        let mut ordered = Vec::new();
        let mut queue = VecDeque::new();

        queue.push_back(entry_file.to_string());
        visited.insert(entry_file.to_string());

        // Regex matching static imports: from "./foo.js" or import "./foo.js"
        // Also matches bare `import "./..."`
        let import_re = Regex::new(r#"(?:from|import)\s*["']\./([^"']+\.js)["']"#).unwrap();

        while let Some(current) = queue.pop_front() {
            let file_path = assets_dir.join(&current);
            if !file_path.exists() {
                continue;
            }

            if let Ok(code) = fs::read_to_string(&file_path) {
                for caps in import_re.captures_iter(&code) {
                    if let Some(dep_match) = caps.get(1) {
                        let dep = dep_match.as_str().to_string();
                        if !visited.contains(&dep) {
                            visited.insert(dep.clone());
                            ordered.push(dep.clone());
                            queue.push_back(dep);
                        }
                    }
                }
            }
        }

        ordered
    }

    /// Extracts entry scripts loaded by an HTML file.
    pub fn extract_entry_scripts(html: &str) -> Vec<String> {
        let script_re = Regex::new(r#"<script\b[^>]*\bsrc=["']/assets/([^"']+\.js)["'][^>]*>"#).unwrap();
        let mut entries = Vec::new();
        for caps in script_re.captures_iter(html) {
            if let Some(m) = caps.get(1) {
                entries.push(m.as_str().to_string());
            }
        }
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_trace_dependencies() {
        let dir = tempdir().unwrap();
        let assets_dir = dir.path();

        fs::write(
            assets_dir.join("main.js"),
            r#"import { a } from "./chunk-a.js"; import "./chunk-b.js";"#,
        )
        .unwrap();
        fs::write(
            assets_dir.join("chunk-a.js"),
            r#"import { c } from "./chunk-c.js";"#,
        )
        .unwrap();
        fs::write(assets_dir.join("chunk-b.js"), "console.log('b');").unwrap();
        fs::write(assets_dir.join("chunk-c.js"), "console.log('c');").unwrap();

        let deps = ModuleGraph::trace_dependencies(assets_dir, "main.js");
        assert_eq!(deps, vec!["chunk-a.js", "chunk-b.js", "chunk-c.js"]);
    }
}
