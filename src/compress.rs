use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct CompressStats {
    pub total_files: usize,
    pub plain_bytes: u64,
    pub zstd_bytes: u64,
    pub brotli_bytes: u64,
    pub gzip_bytes: u64,
}

impl CompressStats {
    pub fn zstd_savings_pct(&self) -> f64 {
        if self.plain_bytes == 0 {
            return 0.0;
        }
        ((self.plain_bytes as f64 - self.zstd_bytes as f64) / self.plain_bytes as f64) * 100.0
    }

    pub fn brotli_savings_pct(&self) -> f64 {
        if self.plain_bytes == 0 {
            return 0.0;
        }
        ((self.plain_bytes as f64 - self.brotli_bytes as f64) / self.plain_bytes as f64) * 100.0
    }

    pub fn gzip_savings_pct(&self) -> f64 {
        if self.plain_bytes == 0 {
            return 0.0;
        }
        ((self.plain_bytes as f64 - self.gzip_bytes as f64) / self.plain_bytes as f64) * 100.0
    }
}

pub struct Compressor {
    nstd_path: Option<PathBuf>,
}

impl Compressor {
    pub fn new(custom_nstd: Option<&Path>) -> Self {
        let nstd_path = Self::find_nstd(custom_nstd);
        Self { nstd_path }
    }

    pub fn nstd_location(&self) -> Option<&Path> {
        self.nstd_path.as_deref()
    }

    /// Locate `nstd` binary using common search heuristics.
    pub fn find_nstd(custom: Option<&Path>) -> Option<PathBuf> {
        if let Some(p) = custom {
            if p.exists() {
                return Some(p.to_path_buf());
            }
        }

        if let Ok(env_path) = std::env::var("NSTD_BIN") {
            let p = PathBuf::from(env_path);
            if p.exists() {
                return Some(p);
            }
        }

        // Candidates relative to current dir or known system paths
        let candidates = [
            PathBuf::from("bin/nstd"),
            PathBuf::from("scripts/bin/nstd"),
            PathBuf::from("../scripts/bin/nstd"),
            PathBuf::from("../../scripts/bin/nstd"),
            PathBuf::from("/home/arielaram/Documents/nstandard/zig-out/bin/nstd"),
            PathBuf::from("/usr/local/bin/nstd"),
        ];

        for c in &candidates {
            if c.exists() {
                return Some(c.clone());
            }
        }

        // Try PATH
        if let Ok(output) = Command::new("which").arg("nstd").output() {
            if output.status.success() {
                let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !p.is_empty() && Path::new(&p).exists() {
                    return Some(PathBuf::from(p));
                }
            }
        }

        None
    }

    /// Precompress a file with Zstandard, Brotli, and Gzip, and verify checksum integrity.
    pub fn precompress_file(&self, path: &Path, level: i32) -> Result<(u64, u64, u64, u64), String> {
        let plain = fs::read(path).map_err(|e| format!("Failed to read {:?}: {}", path, e))?;
        let plain_len = plain.len() as u64;

        let zst_out = path.with_extension(format!(
            "{}.zst",
            path.extension().and_then(|e| e.to_str()).unwrap_or("")
        ));
        let br_out = path.with_extension(format!(
            "{}.br",
            path.extension().and_then(|e| e.to_str()).unwrap_or("")
        ));
        let gz_out = path.with_extension(format!(
            "{}.gz",
            path.extension().and_then(|e| e.to_str()).unwrap_or("")
        ));

        // 1. Compress
        let (zst_bytes, br_bytes, gz_bytes) = if let Some(nstd) = &self.nstd_path {
            Self::compress_via_nstd(nstd, path, &zst_out, &br_out, &gz_out, level)?
        } else {
            Self::compress_via_native(&plain, &zst_out, &br_out, &gz_out, level)?
        };

        // 2. Hash-verify decompression
        Self::verify_file_integrity(&plain, &zst_out, &br_out, &gz_out)?;

        Ok((plain_len, zst_bytes, br_bytes, gz_bytes))
    }

    fn compress_via_nstd(
        nstd: &Path,
        input: &Path,
        zst_out: &Path,
        br_out: &Path,
        gz_out: &Path,
        level: i32,
    ) -> Result<(u64, u64, u64), String> {
        let lvl_flag = format!("-{}", level.clamp(1, 19));

        // Zstandard
        let status_z = Command::new(nstd)
            .args(["-z", &lvl_flag, input.to_str().unwrap(), "-o", zst_out.to_str().unwrap()])
            .status()
            .map_err(|e| format!("nstd zstd error: {}", e))?;
        if !status_z.success() {
            return Err(format!("nstd zstd failed on {:?}", input));
        }

        // Brotli
        let status_b = Command::new(nstd)
            .args(["-b", &lvl_flag, input.to_str().unwrap(), "-o", br_out.to_str().unwrap()])
            .status()
            .map_err(|e| format!("nstd brotli error: {}", e))?;
        if !status_b.success() {
            return Err(format!("nstd brotli failed on {:?}", input));
        }

        // Gzip
        let status_g = Command::new(nstd)
            .args(["-g", &lvl_flag, input.to_str().unwrap(), "-o", gz_out.to_str().unwrap()])
            .status()
            .map_err(|e| format!("nstd gzip error: {}", e))?;
        if !status_g.success() {
            return Err(format!("nstd gzip failed on {:?}", input));
        }

        let z_len = fs::metadata(zst_out).map(|m| m.len()).unwrap_or(0);
        let b_len = fs::metadata(br_out).map(|m| m.len()).unwrap_or(0);
        let g_len = fs::metadata(gz_out).map(|m| m.len()).unwrap_or(0);

        Ok((z_len, b_len, g_len))
    }

    fn compress_via_native(
        plain: &[u8],
        zst_out: &Path,
        br_out: &Path,
        gz_out: &Path,
        level: i32,
    ) -> Result<(u64, u64, u64), String> {
        // Zstandard
        let zstd_data = zstd::encode_all(plain, level.clamp(1, 19))
            .map_err(|e| format!("Native zstd error: {}", e))?;
        fs::write(zst_out, &zstd_data).map_err(|e| e.to_string())?;

        // Brotli
        let mut br_data = Vec::new();
        {
            let mut writer = brotli::CompressorWriter::new(&mut br_data, 4096, level.clamp(1, 11) as u32, 22);
            writer.write_all(plain).map_err(|e| e.to_string())?;
        }
        fs::write(br_out, &br_data).map_err(|e| e.to_string())?;

        // Gzip
        let mut gz_encoder = GzEncoder::new(Vec::new(), Compression::new(level.clamp(1, 9) as u32));
        gz_encoder.write_all(plain).map_err(|e| e.to_string())?;
        let gz_data = gz_encoder.finish().map_err(|e| e.to_string())?;
        fs::write(gz_out, &gz_data).map_err(|e| e.to_string())?;

        Ok((zstd_data.len() as u64, br_data.len() as u64, gz_data.len() as u64))
    }

    /// Verifies that decompressing .zst, .br, and .gz produces exact byte-for-byte original match.
    pub fn verify_file_integrity(
        plain: &[u8],
        zst_path: &Path,
        br_path: &Path,
        gz_path: &Path,
    ) -> Result<(), String> {
        let expected_hash = Sha256::digest(plain);

        // 1. Verify Zstd
        let zst_bytes = fs::read(zst_path).map_err(|e| e.to_string())?;
        let decompressed_zst = zstd::decode_all(&zst_bytes[..])
            .map_err(|e| format!("Corrupt Zstd archive {:?}: {}", zst_path, e))?;
        if Sha256::digest(&decompressed_zst) != expected_hash {
            return Err(format!("Hash mismatch on decompressed {:?}", zst_path));
        }

        // 2. Verify Brotli
        let br_bytes = fs::read(br_path).map_err(|e| e.to_string())?;
        let mut decompressed_br = Vec::new();
        brotli::BrotliDecompress(&mut &br_bytes[..], &mut decompressed_br)
            .map_err(|e| format!("Corrupt Brotli archive {:?}: {}", br_path, e))?;
        if Sha256::digest(&decompressed_br) != expected_hash {
            return Err(format!("Hash mismatch on decompressed {:?}", br_path));
        }

        // 3. Verify Gzip
        let gz_bytes = fs::read(gz_path).map_err(|e| e.to_string())?;
        let mut gz_decoder = GzDecoder::new(&gz_bytes[..]);
        let mut decompressed_gz = Vec::new();
        gz_decoder
            .read_to_end(&mut decompressed_gz)
            .map_err(|e| format!("Corrupt Gzip archive {:?}: {}", gz_path, e))?;
        if Sha256::digest(&decompressed_gz) != expected_hash {
            return Err(format!("Hash mismatch on decompressed {:?}", gz_path));
        }

        Ok(())
    }

    /// Precompress all matching files in target directories in parallel with Rayon.
    pub fn precompress_all(
        &self,
        files: &[PathBuf],
        level: i32,
    ) -> Result<CompressStats, Vec<String>> {
        let results: Vec<Result<(u64, u64, u64, u64), String>> = files
            .par_iter()
            .map(|f| self.precompress_file(f, level))
            .collect();

        let mut stats = CompressStats {
            total_files: files.len(),
            ..Default::default()
        };
        let mut errors = Vec::new();

        for res in results {
            match res {
                Ok((plain, zst, br, gz)) => {
                    stats.plain_bytes += plain;
                    stats.zstd_bytes += zst;
                    stats.brotli_bytes += br;
                    stats.gzip_bytes += gz;
                }
                Err(err) => errors.push(err),
            }
        }

        if errors.is_empty() {
            Ok(stats)
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_native_precompress_and_verify() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.js");
        fs::write(&file, b"console.log('Testing precompression integrity');").unwrap();

        let compressor = Compressor::new(None);
        let res = compressor.precompress_file(&file, 9);
        assert!(res.is_ok(), "Precompress failed: {:?}", res);

        let (plain, zst, br, gz) = res.unwrap();
        assert!(plain > 0);
        assert!(zst > 0);
        assert!(br > 0);
        assert!(gz > 0);
    }
}
