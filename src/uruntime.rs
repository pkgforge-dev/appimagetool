//! Resolve the uruntime binary (cache or download) and patch its ELF sections
//! to carry the build's `upd_info`, env vars, and mount-mode marker.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::elf;
use crate::error::{Error, Result};
use crate::util;

/// Marker uruntime ships in `.rodata` to advertise its mount mode.
const MOUNT_MARKER: &[u8] = b"URUNTIME_MOUNT=";

/// Default uruntime download URL pattern. Pinned for reproducible builds;
/// override with `--runtime-url` / `URUNTIME_LINK` to track a different
/// release. `{arch}` gets replaced with the target architecture.
const DEFAULT_URL_TEMPLATE: &str = "https://github.com/VHSgunzo/uruntime/releases/download/v0.5.8/uruntime-appimage-dwarfs-lite-{arch}";

/// Ensure a runtime binary is available. Returns the path to the runtime.
///
/// If `config.runtime` is set and the file exists, use that.
/// Otherwise, download from `config.runtime_url` (or the default URL) to `config.tmpdir`.
pub fn resolve_runtime(config: &Config) -> Result<PathBuf> {
    // If user provided a runtime path, use it directly
    if let Some(ref path) = config.runtime {
        if path.exists() {
            return Ok(path.clone());
        }
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("runtime not found at {}", path.display()),
        )));
    }

    // Cache in tmpdir (include arch in name for cross-builds).
    let cached = config
        .tmpdir
        .join(format!("uruntime-{}", config.appimage_arch));
    let url = config
        .runtime_url
        .as_deref()
        .unwrap_or(DEFAULT_URL_TEMPLATE)
        .replace("{arch}", &config.appimage_arch);
    util::ensure_cached_binary(&cached, &url, "uruntime")?;

    // Return a per-process working copy so concurrent builds don't clobber
    // each other and the cached original stays pristine.
    let work = util::process_unique_path(
        &config.tmpdir,
        &format!("uruntime-{}.work", config.appimage_arch),
    );
    std::fs::copy(&cached, &work)?;
    util::set_executable(&work)?;
    Ok(work)
}

/// Configure the runtime: write ELF sections for update info and env vars,
/// and optionally patch runtime behavior.
pub fn configure_runtime(
    runtime_path: &Path,
    update_info: Option<&str>,
    env_vars: &[String],
    keep_mount: bool,
) -> Result<()> {
    let mut data = std::fs::read(runtime_path)?;

    // Write update info into .upd_info section
    if let Some(upinfo) = update_info {
        crate::log_info!("Adding update information to runtime...");
        elf::write_section(&mut data, ".upd_info", upinfo.as_bytes())?;
    }

    // Write env vars into .envs section
    if !env_vars.is_empty() {
        crate::log_info!("Adding environment variables to runtime...");
        let env_data: String = env_vars.join("\n");
        elf::write_section(&mut data, ".envs", env_data.as_bytes())?;
    }

    if keep_mount {
        crate::log_info!("Setting runtime to keep mount point...");
        patch_keep_mount(&mut data)?;
    }

    std::fs::write(runtime_path, data)?;
    Ok(())
}

/// Rewrite every `URUNTIME_MOUNT=<digit>` occurrence in the runtime to
/// `URUNTIME_MOUNT=0` (keep-mount). The marker lives in `.rodata` (a string
/// literal in the runtime source), not in `.upd_info` or `.envs`, so we scan
/// the whole binary. The rewrite is byte-for-byte (one digit → '0'), so no
/// ELF reflow is needed. Returns an error if the marker is absent entirely —
/// silently leaving the runtime unmodified would mask a real version mismatch.
fn patch_keep_mount(data: &mut [u8]) -> Result<()> {
    let mut patched = 0usize;
    let mut start = 0;
    while let Some(pos) = find_subslice(&data[start..], MOUNT_MARKER) {
        let digit_idx = start + pos + MOUNT_MARKER.len();
        if let Some(b) = data.get_mut(digit_idx)
            && b.is_ascii_digit()
        {
            *b = b'0';
            patched += 1;
        }
        start = digit_idx;
    }
    if patched == 0 {
        return Err(Error::Config(
            "could not patch URUNTIME_MOUNT: marker not found in runtime\n  \
             hint: this runtime build may not support URUNTIME_PRELOAD; \
             try a newer uruntime release"
                .to_string(),
        ));
    }
    Ok(())
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_keep_mount_rewrites_digit() {
        let mut data = Vec::new();
        data.extend_from_slice(&[0u8; 64]);
        data.extend_from_slice(b"URUNTIME_MOUNT=3");
        data.extend_from_slice(&[0u8; 32]);
        let digit_idx = 64 + MOUNT_MARKER.len();

        patch_keep_mount(&mut data).unwrap();
        assert_eq!(data[digit_idx], b'0');
    }

    #[test]
    fn patch_keep_mount_rewrites_all_occurrences() {
        let prefix_a = b"URUNTIME_MOUNT=3";
        let gap = [0u8; 16];
        let prefix_b = b"URUNTIME_MOUNT=2";
        let mut data = Vec::new();
        data.extend_from_slice(prefix_a);
        data.extend_from_slice(&gap);
        data.extend_from_slice(prefix_b);
        let first = MOUNT_MARKER.len();
        let second = prefix_a.len() + gap.len() + MOUNT_MARKER.len();

        patch_keep_mount(&mut data).unwrap();
        assert_eq!(data[first], b'0');
        assert_eq!(data[second], b'0');
    }

    #[test]
    fn patch_keep_mount_idempotent_on_zero() {
        let mut data = b"...URUNTIME_MOUNT=0...".to_vec();
        let before = data.clone();
        patch_keep_mount(&mut data).unwrap();
        assert_eq!(data, before);
    }

    #[test]
    fn patch_keep_mount_skips_non_digit_followers() {
        // The runtime ships a format string like "URUNTIME_MOUNT==0" alongside
        // the real marker. Without a real digit in any occurrence, we must
        // report failure rather than silently no-op.
        let mut data = b"URUNTIME_MOUNT==0".to_vec();
        assert!(patch_keep_mount(&mut data).is_err());
    }

    #[test]
    fn patch_keep_mount_errors_when_absent() {
        let mut data = vec![0u8; 256];
        assert!(patch_keep_mount(&mut data).is_err());
    }
}
