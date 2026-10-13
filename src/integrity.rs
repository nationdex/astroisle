use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use regex::Regex;
use sha2::{Digest, Sha256, Sha384, Sha512};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Integrity;

impl Integrity {
    /// Compute Subresource Integrity (SRI) string for bytes.
    pub fn compute_sri(bytes: &[u8], algorithm: &str) -> String {
        match algorithm.to_lowercase().as_str() {
            "sha256" => {
                let mut hasher = Sha256::new();
                hasher.update(bytes);
                let digest = hasher.finalize();
                format!("sha256-{}", BASE64.encode(digest))
            }
            "sha512" => {
                let mut hasher = Sha512::new();
                hasher.update(bytes);
                let digest = hasher.finalize();
                format!("sha512-{}", BASE64.encode(digest))
            }
            _ => {
                // Default to SHA-384 as per W3C SRI recommendation
                let mut hasher = Sha384::new();
                hasher.update(bytes);
                let digest = hasher.finalize();
                format!("sha384-{}", BASE64.encode(digest))
            }
        }
    }

    /// Compute SRI for a file on disk.
    pub fn compute_file_sri(path: &Path, algorithm: &str) -> std::io::Result<String> {
        let bytes = fs::read(path)?;
        Ok(Self::compute_sri(&bytes, algorithm))
    }

    /// Hashes unhashed boot scripts (e.g. `boot.js` -> `assets/boot-<hash>.js`),
    /// moves the file, and replaces references in HTML.
    pub fn hash_and_relocate_boot_script(
        dist_dir: &Path,
        html_content: &mut String,
        script_name: &str,
    ) -> Result<Option<PathBuf>, String> {
        let boot_path = dist_dir.join(script_name);
        if !boot_path.exists() {
            return Ok(None);
        }

        let body = fs::read(&boot_path).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        hasher.update(&body);
        let hash_hex = format!("{:x}", hasher.finalize());
        let short_hash = &hash_hex[..8];

        let ext = boot_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("js");
        let stem = boot_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("boot");
        let hashed_filename = format!("{}-{}.{}", stem, short_hash, ext);

        let assets_dir = dist_dir.join("assets");
        if !assets_dir.exists() {
            fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;
        }

        let new_path = assets_dir.join(&hashed_filename);
        fs::write(&new_path, &body).map_err(|e| e.to_string())?;
        let _ = fs::remove_file(&boot_path);

        let old_ref = format!("/{}", script_name);
        let new_ref = format!("/assets/{}", hashed_filename);
        *html_content = html_content.replace(&old_ref, &new_ref);

        Ok(Some(new_path))
    }

    /// Injects SRI integrity attributes to all local `<script>` and `<link>` tags in HTML.
    pub fn pin_html_resources(
        dist_dir: &Path,
        html_content: &str,
        algorithm: &str,
    ) -> (String, Vec<String>) {
        let tag_re = Regex::new(r#"<(script|link)\b[^>]*>"#).unwrap();
        let ref_re = Regex::new(r#"\b(?:src|href)=["'](/[^"']+\.(?:js|css))["']"#).unwrap();
        let mut unpinned = Vec::new();

        let pinned = tag_re.replace_all(html_content, |caps: &regex::Captures| {
            let tag = &caps[0];
            if tag.contains("integrity=") {
                return tag.to_string();
            }

            if let Some(ref_caps) = ref_re.captures(tag) {
                if let Some(matched_ref) = ref_caps.get(1) {
                    let rel_path = matched_ref.as_str().trim_start_matches('/');
                    let full_path = dist_dir.join(rel_path);

                    if full_path.exists() {
                        if let Ok(sri_value) = Self::compute_file_sri(&full_path, algorithm) {
                            let end_re = Regex::new(r#"\s*/?>$"#).unwrap();
                            return end_re
                                .replace(tag, |end_caps: &regex::Captures| {
                                    format!(r#" integrity="{}"{}"#, sri_value, &end_caps[0])
                                })
                                .to_string();
                        }
                    } else {
                        unpinned.push(format!("File not found: {}", rel_path));
                    }
                }
            }

            tag.to_string()
        });

        // Audit for any unpinned local script/link tags
        let unpinned_re =
            Regex::new(r#"<(?:script|link)\b[^>]*(?:src|href)=["']/[^"']+\.(?:js|css)["'][^>]*>"#)
                .unwrap();
        for m in unpinned_re.captures_iter(&pinned) {
            let full_tag = &m[0];
            if !full_tag.contains("integrity=") {
                unpinned.push(full_tag.to_string());
            }
        }

        (pinned.to_string(), unpinned)
    }

    /// Injects `<link rel="modulepreload">` tags before the entry script tag.
    pub fn inject_module_preloads(
        html_content: &str,
        deps: &[String],
        dist_dir: &Path,
        algorithm: &str,
    ) -> String {
        if deps.is_empty() {
            return html_content.to_string();
        }

        let preload_tags: Vec<String> = deps
            .iter()
            .map(|dep| {
                let full_path = dist_dir.join("assets").join(dep);
                let integrity_attr = if full_path.exists() {
                    Self::compute_file_sri(&full_path, algorithm)
                        .map(|sri| format!(r#" integrity="{}""#, sri))
                        .unwrap_or_default()
                } else {
                    String::new()
                };

                format!(
                    r#"<link rel="modulepreload" crossorigin href="/assets/{}"{} />"#,
                    dep, integrity_attr
                )
            })
            .collect();

        let joined = preload_tags.join("");

        // Find the first <script type="module"> tag and prepend the preload links
        let script_entry_re = Regex::new(r#"(<script\b[^>]*type=["']module["'][^>]*>)"#).unwrap();
        if let Some(mat) = script_entry_re.find(html_content) {
            let idx = mat.start();
            let mut result = String::with_capacity(html_content.len() + joined.len());
            result.push_str(&html_content[..idx]);
            result.push_str(&joined);
            result.push_str(&html_content[idx..]);
            result
        } else {
            html_content.to_string()
        }
    }

    /// Audits directory for leaked source maps or sourceMappingURL directives.
    pub fn check_source_map_leaks(assets_dir: &Path) -> Result<(), Vec<String>> {
        let mut leaks = Vec::new();
        if !assets_dir.exists() {
            return Ok(());
        }

        if let Ok(entries) = fs::read_dir(assets_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "map") {
                    leaks.push(format!("Leaked .map file: {:?}", path.file_name().unwrap()));
                } else if path.extension().is_some_and(|ext| ext == "js" || ext == "css") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if content.contains("sourceMappingURL=") {
                            leaks.push(format!(
                                "sourceMappingURL in {:?}",
                                path.file_name().unwrap()
                            ));
                        }
                    }
                }
            }
        }

        if leaks.is_empty() {
            Ok(())
        } else {
            Err(leaks)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_compute_sri() {
        let data = b"console.log('hello');";
        let sri = Integrity::compute_sri(data, "sha384");
        assert!(sri.starts_with("sha384-"));
    }

    #[test]
    fn test_pin_html_resources() {
        let dir = tempdir().unwrap();
        let assets = dir.path().join("assets");
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("app.js"), "alert(1);").unwrap();

        let html = r#"<html><head><script type="module" src="/assets/app.js"></script></head></html>"#;
        let (pinned, unpinned) = Integrity::pin_html_resources(dir.path(), html, "sha384");
        assert!(unpinned.is_empty(), "Unpinned errors: {:?}", unpinned);
        assert!(pinned.contains(r#"integrity="sha384-"#));
    }
}
