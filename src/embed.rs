//! Helper binaries compiled into this build by the optional `embed-uruntime`
//! and `embed-mkdwarfs` features, and the license reporting that goes with them.
//!
//! A default build embeds nothing: every accessor returns `None` and the
//! download paths in [`crate::uruntime`] and [`crate::dwarfs`] remain the only
//! source of helpers. Blobs are architecture-specific, so a build that embeds
//! one for its own target still falls back to downloading when the caller asks
//! for a different `--appimage-arch`.

use std::path::Path;

use crate::error::Result;
use crate::util;

#[cfg(feature = "embed-uruntime")]
const URUNTIME_BLOB: Option<&[u8]> = Some(include_bytes!(env!("APPIMAGETOOL_EMBED_URUNTIME")));
#[cfg(not(feature = "embed-uruntime"))]
const URUNTIME_BLOB: Option<&[u8]> = None;

#[cfg(feature = "embed-mkdwarfs")]
const MKDWARFS_BLOB: Option<&[u8]> = Some(include_bytes!(env!("APPIMAGETOOL_EMBED_MKDWARFS")));
#[cfg(not(feature = "embed-mkdwarfs"))]
const MKDWARFS_BLOB: Option<&[u8]> = None;

#[cfg(any(feature = "embed-uruntime", feature = "embed-mkdwarfs"))]
const EMBED_ARCH: &str = env!("APPIMAGETOOL_EMBED_ARCH");
#[cfg(not(any(feature = "embed-uruntime", feature = "embed-mkdwarfs")))]
const EMBED_ARCH: &str = "";

fn blob_for(blob: Option<&'static [u8]>, arch: &str) -> Option<&'static [u8]> {
    (arch == EMBED_ARCH).then_some(blob).flatten()
}

/// Embedded uruntime for `arch`, or `None` when this build embeds none or was
/// built for a different architecture.
pub fn uruntime(arch: &str) -> Option<&'static [u8]> {
    blob_for(URUNTIME_BLOB, arch)
}

/// Embedded `mkdwarfs` for `arch`, or `None` when this build embeds none or was
/// built for a different architecture.
pub fn mkdwarfs(arch: &str) -> Option<&'static [u8]> {
    blob_for(MKDWARFS_BLOB, arch)
}

/// Write an embedded blob to `dest` and mark it executable. The bytes land on a
/// per-process staging path and are renamed into place, so concurrent builds
/// sharing a TMPDIR cannot observe a partially written helper.
pub fn materialize(bytes: &[u8], dest: &Path) -> Result<()> {
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    let name = dest.file_name().unwrap_or_default().to_string_lossy();
    let staging = util::process_unique_path(parent, &format!("{name}.partial"));

    std::fs::write(&staging, bytes)?;
    util::set_executable(&staging)?;
    if let Err(e) = std::fs::rename(&staging, dest) {
        let _ = std::fs::remove_file(&staging);
        return Err(e.into());
    }
    Ok(())
}

/// SPDX expression for the effective license of this binary.
///
/// appimagetool's own source is MIT in every configuration. `embed-mkdwarfs`
/// links in the GPL-3.0 DwarFS writer, which makes the resulting artifact a
/// GPL-3.0 work even though the source license is unchanged.
pub fn effective_license() -> &'static str {
    if cfg!(feature = "embed-mkdwarfs") {
        "GPL-3.0-or-later"
    } else {
        "MIT"
    }
}

/// License text for this build: appimagetool's own terms, the notices its
/// components require, and the source-availability directions that apply when a
/// GPL-3.0 component is embedded.
pub fn notices() -> String {
    let mut out = format!(
        "appimagetool {}\nEffective license of this binary: {}\n\nHelper binaries:\n",
        env!("CARGO_PKG_VERSION"),
        effective_license()
    );
    out.push_str(&format!(
        "  uruntime  {}\n",
        if cfg!(feature = "embed-uruntime") {
            "embedded in this binary at build time"
        } else {
            "downloaded on demand"
        }
    ));
    out.push_str(&format!(
        "  mkdwarfs  {}\n",
        if cfg!(feature = "embed-mkdwarfs") {
            "embedded in this binary at build time"
        } else {
            "taken from $PATH, or downloaded on demand"
        }
    ));

    section(&mut out, "appimagetool", include_str!("../LICENSE"));

    // Only the components this binary actually carries are listed. A helper that
    // is downloaded at run time is distributed by its own upstream, and the
    // AppImages this tool produces are distributed by whoever builds them.
    if cfg!(feature = "embed-uruntime") {
        section(
            &mut out,
            "uruntime, embedded in this binary",
            include_str!("../licenses/uruntime-MIT.txt"),
        );
        section(
            &mut out,
            "DwarFS reader and extractor, embedded in uruntime",
            include_str!("../licenses/dwarfs-MIT.txt"),
        );
    }

    if cfg!(feature = "embed-mkdwarfs") {
        section(
            &mut out,
            "mkdwarfs, embedded in this binary",
            include_str!("../licenses/mkdwarfs-GPL-3.0.txt"),
        );
    }
    out
}

fn section(out: &mut String, title: &str, body: &str) {
    out.push_str(&format!("\n=== {title} ===\n\n"));
    out.push_str(body.trim_end());
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_is_rejected_for_mismatched_arch() {
        let bytes: &'static [u8] = b"payload";
        assert_eq!(blob_for(Some(bytes), EMBED_ARCH), Some(bytes));
        assert_eq!(blob_for(Some(bytes), "definitely-not-a-real-arch"), None);
    }

    #[test]
    fn default_build_embeds_nothing() {
        // Guards against an accessor silently returning a blob for the wrong
        // target; with no feature enabled there is nothing to return at all.
        if cfg!(not(feature = "embed-uruntime")) {
            assert!(uruntime(EMBED_ARCH).is_none());
        }
        if cfg!(not(feature = "embed-mkdwarfs")) {
            assert!(mkdwarfs(EMBED_ARCH).is_none());
        }
    }

    #[test]
    fn notices_cover_the_embedded_components() {
        let text = notices();
        assert!(text.contains("appimagetool"));
        // A notice appears only when the component it covers is embedded here.
        assert_eq!(text.contains("VHSgunzo"), cfg!(feature = "embed-uruntime"));
        assert_eq!(
            text.contains("GNU General Public License"),
            cfg!(feature = "embed-mkdwarfs")
        );
    }

    #[test]
    fn effective_license_tracks_the_gpl_feature() {
        if cfg!(feature = "embed-mkdwarfs") {
            assert_eq!(effective_license(), "GPL-3.0-or-later");
        } else {
            assert_eq!(effective_license(), "MIT");
        }
    }

    #[test]
    fn materialize_writes_an_executable_file() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("appimagetool-embed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("helper");

        materialize(b"\x7fELF-ish", &dest).unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), b"\x7fELF-ish");
        assert_ne!(
            std::fs::metadata(&dest).unwrap().permissions().mode() & 0o111,
            0
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
