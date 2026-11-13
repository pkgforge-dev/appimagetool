//! Build the small SquashFS metadata sidecar placed between the runtime and
//! the DwarFS payload.
//!
//! Many thumbnailers and desktop-integration tools only know how to read a
//! SquashFS at the end of the runtime ELF. The sidecar carries `.DirIcon`, the
//! main `.desktop` entry, and a stub `AppRun` so those tools work against a
//! DwarFS AppImage. The runtime skips this image and mounts the following
//! DwarFS payload.
//!
//! The image is always padded to [`SIDECAR_SLOT_BYTES`]. Keeping the slot size
//! constant keeps the DwarFS payload at a stable offset across releases, which
//! preserves zsync delta-update efficiency.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use backhand::compression::{CompressionOptions, Compressor, Zstd};
use backhand::{BackhandError, FilesystemCompressor, FilesystemWriter, NodeHeader};

use crate::error::{Error, Result};

/// zstd level used for the sidecar.
const SIDECAR_ZSTD_LEVEL: u32 = 22;

/// Fixed size of the metadata slot between the runtime and the DwarFS payload.
///
/// The image is zero-padded to this size. This value must not exceed
/// uruntime's `METADATA_PAYLOAD_SCAN_LIMIT`; the patched runtime uses a larger
/// limit so a 128 KiB slot can always be skipped.
pub const SIDECAR_SLOT_BYTES: usize = 128 * 1024;

/// Stub `AppRun` for tools that mount or extract the metadata image instead of
/// using uruntime. It must never be mistaken for the real DwarFS payload.
const DUMMY_APPRUN: &[u8] = b"#!/bin/sh\n\
>&2 echo 'This is not the real AppImage payload, this is a DwarFS AppImage, please use dwarfs to mount it instead'\n\
exit 1\n";

/// Build a padded metadata SquashFS image containing `.DirIcon`, the main
/// desktop entry, and a stub `AppRun` from `appdir`.
///
/// `desktop_path` must be the final desktop file (after X-AppImage metadata
/// has been added), so the sidecar advertises the same metadata as the real
/// payload.
pub fn build(appdir: &Path, desktop_path: &Path) -> Result<Vec<u8>> {
    let dir_icon = std::fs::read(appdir.join(".DirIcon"))?;
    let desktop = std::fs::read(desktop_path)?;
    let desktop_name = desktop_path
        .file_name()
        .ok_or_else(|| Error::MetadataSidecar("desktop entry has no file name".to_string()))?
        .to_owned();

    let mut fs = FilesystemWriter::default();
    // A fixed timestamp and root ownership keep the sidecar reproducible.
    fs.set_time(0);
    fs.set_only_root_id();
    fs.set_root_mode(0o755);
    fs.set_root_uid(0);
    fs.set_root_gid(0);
    fs.set_no_padding();
    let compressor = FilesystemCompressor::new(
        Compressor::Zstd,
        Some(CompressionOptions::Zstd(Zstd {
            compression_level: SIDECAR_ZSTD_LEVEL,
        })),
    )
    .map_err(metadata_error)?;
    fs.set_compressor(compressor);
    // The zstd level is a build-time choice; readers only need the compressor
    // id, so don't spend bytes storing the options in the image.
    fs.set_emit_compression_options(false);

    let file_header = NodeHeader::new(0o644, 0, 0, 0);
    fs.push_file(Cursor::new(dir_icon), Path::new(".DirIcon"), file_header)
        .map_err(metadata_error)?;
    fs.push_file(Cursor::new(desktop), Path::new(&desktop_name), file_header)
        .map_err(metadata_error)?;
    fs.push_file(
        Cursor::new(DUMMY_APPRUN),
        Path::new("AppRun"),
        NodeHeader::new(0o755, 0, 0, 0),
    )
    .map_err(metadata_error)?;

    let mut output = Cursor::new(Vec::new());
    let (superblock, _) = fs.write(&mut output).map_err(metadata_error)?;
    let mut image = output.into_inner();
    let actual_size = usize::try_from(superblock.bytes_used).map_err(|_| {
        Error::MetadataSidecar("metadata image size does not fit usize".to_string())
    })?;

    if actual_size > SIDECAR_SLOT_BYTES {
        return Err(Error::MetadataSidecarTooLarge {
            size: actual_size,
            limit: SIDECAR_SLOT_BYTES,
        });
    }

    image.resize(SIDECAR_SLOT_BYTES, 0);
    crate::log_debug!(
        "metadata sidecar: {} bytes, padded to {} bytes",
        actual_size,
        SIDECAR_SLOT_BYTES
    );
    Ok(image)
}

/// Write `runtime || sidecar` to `dest` for use as `mkdwarfs --header`.
pub fn write_header(dest: &Path, runtime: &Path, sidecar: &[u8]) -> Result<()> {
    use std::io::Write;

    let runtime_bytes = std::fs::read(runtime)?;
    let mut file = std::fs::File::create(dest)?;
    file.write_all(&runtime_bytes)?;
    file.write_all(sidecar)?;
    file.flush()?;
    Ok(())
}

/// Temporary header path for a single build.
pub fn header_path(tmpdir: &Path) -> PathBuf {
    crate::util::process_unique_path(tmpdir, "appimage-header")
}

fn metadata_error(error: BackhandError) -> Error {
    Error::MetadataSidecar(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "appimagetool-metadata-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn builds_fixed_size_squashfs_with_expected_entries() {
        let dir = temp_dir("build");
        std::fs::write(dir.join(".DirIcon"), b"fake png bytes").unwrap();
        std::fs::write(dir.join("TestApp.desktop"), b"[Desktop Entry]\nName=Test\n").unwrap();

        let image = build(&dir, &dir.join("TestApp.desktop")).unwrap();
        assert_eq!(image.len(), SIDECAR_SLOT_BYTES);
        assert_eq!(&image[..4], b"hsqs");

        let bytes_used = u64::from_le_bytes(image[40..48].try_into().unwrap());
        assert!(bytes_used <= SIDECAR_SLOT_BYTES as u64);

        let reader =
            backhand::FilesystemReader::from_reader(Cursor::new(image.as_slice())).unwrap();
        let paths: Vec<_> = reader.files().map(|node| node.fullpath.clone()).collect();
        assert!(paths.iter().any(|path| path == Path::new("/.DirIcon")));
        assert!(
            paths
                .iter()
                .any(|path| path == Path::new("/TestApp.desktop"))
        );
        assert!(paths.iter().any(|path| path == Path::new("/AppRun")));

        let apprun = reader
            .files()
            .find(|node| node.fullpath == Path::new("/AppRun"))
            .unwrap();
        assert_eq!(apprun.header.permissions, 0o755);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_reproducible() {
        let dir = temp_dir("repro");
        std::fs::write(dir.join(".DirIcon"), b"fake png bytes").unwrap();
        std::fs::write(dir.join("TestApp.desktop"), b"[Desktop Entry]\nName=Test\n").unwrap();

        let first = build(&dir, &dir.join("TestApp.desktop")).unwrap();
        let second = build(&dir, &dir.join("TestApp.desktop")).unwrap();
        assert_eq!(first, second);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_metadata_larger_than_slot() {
        let dir = temp_dir("large");
        // Pseudo-random bytes resist zstd better than a repeating pattern.
        let mut state = 0x9e37_79b9_u32;
        let large: Vec<u8> = (0..SIDECAR_SLOT_BYTES * 2)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state & 0xff) as u8
            })
            .collect();
        std::fs::write(dir.join(".DirIcon"), &large).unwrap();
        std::fs::write(dir.join("TestApp.desktop"), b"[Desktop Entry]\nName=Test\n").unwrap();

        let error = build(&dir, &dir.join("TestApp.desktop")).unwrap_err();
        assert!(matches!(error, Error::MetadataSidecarTooLarge { .. }));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
