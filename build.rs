//! Resolves the helper binaries that the optional `embed-uruntime` and
//! `embed-mkdwarfs` features compile into the crate.
//!
//! With neither feature enabled this script does nothing at all, so a default
//! build needs no network access and stays buildable on docs.rs and inside
//! distro build sandboxes that isolate the network.
//!
//! When a feature is enabled, its blob is resolved in this order:
//!
//! 1. an explicit local path, from `URUNTIME_EMBED_PATH` / `MKDWARFS_EMBED_PATH`
//!    or a matching file in `APPIMAGETOOL_EMBED_DIR`
//! 2. a digest-valid file in the in-tree `.embed-cache/` directory
//! 3. a download from the pinned upstream URL
//!
//! Every candidate is verified against the SHA-256 pinned in `src/pinned.rs`,
//! whichever route produced it. Step 1 exists so packagers who build offline can
//! pre-fetch the artifact and never reach step 3.

#[allow(dead_code)]
mod pinned {
    include!("src/pinned.rs");
}

use std::path::{Path, PathBuf};
use std::process::Command;

/// Upper bound on a downloaded helper. The largest pinned asset is about
/// 2.7 MiB, so this leaves generous headroom while still keeping a hostile or
/// misconfigured endpoint from filling the build host's disk.
const MAX_BLOB_BYTES: u64 = 16 * 1024 * 1024;

/// One embeddable helper binary.
struct Blob {
    /// Short name used for cache filenames and the generated env var.
    kind: &'static str,
    /// Cargo feature that requests this blob.
    feature: &'static str,
    /// Env var holding an explicit path to a pre-fetched copy.
    path_env: &'static str,
    url_template: &'static str,
    checksums: &'static [(&'static str, &'static str)],
}

const BLOBS: &[Blob] = &[
    Blob {
        kind: "uruntime",
        feature: "embed-uruntime",
        path_env: "URUNTIME_EMBED_PATH",
        url_template: pinned::URUNTIME_URL_TEMPLATE,
        checksums: pinned::URUNTIME_CHECKSUMS,
    },
    Blob {
        kind: "mkdwarfs",
        feature: "embed-mkdwarfs",
        path_env: "MKDWARFS_EMBED_PATH",
        url_template: pinned::MKDWARFS_URL_TEMPLATE,
        checksums: pinned::MKDWARFS_CHECKSUMS,
    },
];

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src/pinned.rs");
    for var in [
        "APPIMAGETOOL_EMBED_DIR",
        "APPIMAGETOOL_EMBED_CURL",
        "URUNTIME_EMBED_PATH",
        "MKDWARFS_EMBED_PATH",
    ] {
        println!("cargo::rerun-if-env-changed={var}");
    }

    let requested: Vec<&Blob> = BLOBS
        .iter()
        .filter(|blob| feature_enabled(blob.feature))
        .collect();
    if requested.is_empty() {
        return;
    }

    if feature_enabled("embed-mkdwarfs") {
        println!(
            "cargo::warning=feature `embed-mkdwarfs` embeds dwarfs-universal, which contains \
             the GPL-3.0 mkdwarfs writer. The resulting binary is a GPL-3.0-or-later work and \
             whoever distributes it must keep the Corresponding Source available. See \
             licenses/mkdwarfs-GPL-3.0.txt."
        );
    }

    let arch = target_asset_arch();
    let out_dir = PathBuf::from(required_env("OUT_DIR"));
    let manifest_dir = PathBuf::from(required_env("CARGO_MANIFEST_DIR"));

    for blob in requested {
        match resolve(blob, &arch, &out_dir, &manifest_dir) {
            Ok(path) => println!(
                "cargo::rustc-env=APPIMAGETOOL_EMBED_{}={}",
                blob.kind.to_uppercase(),
                path.display()
            ),
            Err(err) => panic!("failed to embed {}: {err}", blob.kind),
        }
    }

    println!("cargo::rustc-env=APPIMAGETOOL_EMBED_ARCH={arch}");
}

/// Test whether a Cargo feature is active. Uses the documented
/// `CARGO_FEATURE_<NAME>` variables rather than `cfg!`, which reflects how the
/// build script itself was compiled rather than the package's feature set.
fn feature_enabled(feature: &str) -> bool {
    let var = format!("CARGO_FEATURE_{}", feature.to_uppercase().replace('-', "_"));
    std::env::var_os(var).is_some()
}

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// Map the Cargo target triple's architecture onto the `uname -m` spelling used
/// in upstream asset names. `CARGO_CFG_TARGET_ARCH` reports `powerpc64` for both
/// endiannesses, so the endianness has to be consulted separately.
fn target_asset_arch() -> String {
    let arch = required_env("CARGO_CFG_TARGET_ARCH");
    let little_endian = required_env("CARGO_CFG_TARGET_ENDIAN") == "little";
    pinned::target_asset_arch(&arch, little_endian)
}

fn resolve(
    blob: &Blob,
    arch: &str,
    out_dir: &Path,
    manifest_dir: &Path,
) -> Result<PathBuf, String> {
    let expected = pinned::checksum_for(blob.checksums, arch).ok_or_else(|| {
        let supported: Vec<&str> = blob.checksums.iter().map(|(arch, _)| *arch).collect();
        format!(
            "no pinned SHA-256 for `{arch}`, so this target cannot embed {}. \
             Pinned architectures: {}. Build without `--features {}` to download \
             the helper at run time instead.",
            blob.kind,
            supported.join(", "),
            blob.feature
        )
    })?;

    let dest = out_dir.join(format!("{}-{arch}", blob.kind));

    if let Some(local) = explicit_path(blob, arch) {
        let actual = sha256_file(&local)?;
        if actual != expected {
            return Err(format!(
                "{} failed verification: expected {expected}, got {actual}",
                local.display()
            ));
        }
        copy(&local, &dest)?;
        return Ok(dest);
    }

    // Keying the cache filename by digest lets differently pinned checkouts
    // coexist without re-downloading each time the branch changes.
    let cache =
        manifest_dir
            .join(".embed-cache")
            .join(format!("{}-{arch}-{}", blob.kind, &expected[..16]));
    if sha256_file(&cache).as_deref() != Ok(expected) {
        let url = pinned::url_for(blob.url_template, arch);
        download(&url, &cache, blob)?;
        let actual = sha256_file(&cache)?;
        if actual != expected {
            let _ = std::fs::remove_file(&cache);
            return Err(format!(
                "{url} failed verification: expected {expected}, got {actual}"
            ));
        }
    }
    copy(&cache, &dest)?;
    Ok(dest)
}

/// An explicitly supplied local copy, either a direct path or a file named
/// `{kind}-{arch}` inside `APPIMAGETOOL_EMBED_DIR`.
fn explicit_path(blob: &Blob, arch: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(blob.path_env) {
        return Some(PathBuf::from(path));
    }
    let dir = PathBuf::from(std::env::var_os("APPIMAGETOOL_EMBED_DIR")?);
    let candidate = dir.join(format!("{}-{arch}", blob.kind));
    candidate.is_file().then_some(candidate)
}

fn download(url: &str, dest: &Path, blob: &Blob) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let curl = std::env::var_os("APPIMAGETOOL_EMBED_CURL").unwrap_or_else(|| "curl".into());
    let staging = staging_path(dest);

    let status = Command::new(&curl)
        .args(["--silent", "--show-error", "--fail", "--location"])
        .args(["--proto", "=https"])
        .arg("--tlsv1.2")
        .args(["--retry", "3"])
        .args(["--max-time", "300"])
        .arg("--max-filesize")
        .arg(MAX_BLOB_BYTES.to_string())
        .arg("--output")
        .arg(&staging)
        .arg(url)
        .status();

    let failed = |reason: String| {
        let _ = std::fs::remove_file(&staging);
        format!(
            "{reason}\n  \
             hint: set {} to a pre-fetched copy of {url} to build without network access",
            blob.path_env
        )
    };

    match status {
        Ok(status) if status.success() => {}
        Ok(status) => return Err(failed(format!("{:?} exited with {status}", curl))),
        Err(err) => return Err(failed(format!("failed to run {curl:?}: {err}"))),
    }

    std::fs::rename(&staging, dest).map_err(|err| {
        let _ = std::fs::remove_file(&staging);
        format!("failed to install {}: {err}", dest.display())
    })
}

/// Per-process staging path next to `dest`, so concurrent builds sharing the
/// cache directory cannot observe a half-written download.
fn staging_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".partial.{}", std::process::id()));
    dest.with_file_name(name)
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::copy(from, to).map(|_| ()).map_err(|err| {
        format!(
            "failed to copy {} to {}: {err}",
            from.display(),
            to.display()
        )
    })
}

fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};

    let metadata = std::fs::metadata(path)
        .map_err(|err| format!("failed to stat {}: {err}", path.display()))?;
    if metadata.len() > MAX_BLOB_BYTES {
        return Err(format!(
            "{} is {} bytes, over the {MAX_BLOB_BYTES} byte limit",
            path.display(),
            metadata.len()
        ));
    }
    let file = std::fs::File::open(path)
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut std::io::BufReader::new(file), &mut hasher)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    Ok(format!("{:x}", hasher.finalize()))
}
