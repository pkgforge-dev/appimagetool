//! Crate-wide error type. All public fallible APIs return [`Result<T>`].

use std::path::PathBuf;

use thiserror::Error;

/// Errors produced by the AppImage build pipeline.
#[derive(Debug, Error)]
pub enum Error {
    /// The AppDir path is missing or unreadable.
    #[error(
        "AppDir not found: {0}\n  hint: provide a valid AppDir path via --appdir or the APPDIR env var"
    )]
    AppDirNotFound(PathBuf),

    /// No `.desktop` entry found at the AppDir root.
    #[error(
        "No .desktop file found in AppDir\n  \
         hint: place exactly one .desktop file in the AppDir root"
    )]
    NoDesktopEntry,

    /// No `.DirIcon` found at the AppDir root.
    #[error(
        "No .DirIcon found in AppDir\n  \
         hint: add a PNG/SVG icon as .DirIcon in the AppDir root"
    )]
    NoDirIcon,

    /// No `AppRun` entry-point script found at the AppDir root.
    #[error(
        "No AppRun found in AppDir\n  \
         hint: add an executable AppRun script in the AppDir root"
    )]
    NoAppRun,

    /// HTTP download failed after exhausting retries.
    #[error(
        "Failed to download {url}: {reason}\n  hint: check your network connection and the URL"
    )]
    DownloadFailed {
        /// URL the download targeted.
        url: String,
        /// Underlying error reported by the HTTP client.
        reason: String,
    },

    /// The named ELF section was not present in the runtime binary.
    #[error(
        "ELF section '{0}' not found in runtime\n  \
         hint: the runtime binary may be incompatible or corrupted; \
         try a different --runtime or remove the cached download"
    )]
    SectionNotFound(String),

    /// The runtime binary did not parse as a valid ELF or had section
    /// headers pointing outside the file.
    #[error(
        "Runtime is not a valid ELF binary or has corrupt section headers\n  \
         hint: remove the cached runtime and let it re-download, \
         or pass a known-good binary via --runtime"
    )]
    MalformedElf,

    /// Tried to write more bytes into an ELF section than fit.
    #[error(
        "Data too large for ELF section '{name}': {size} > {capacity} bytes\n  \
         hint: shorten the value or use a runtime with larger ELF sections"
    )]
    SectionOverflow {
        /// Name of the section we tried to write.
        name: String,
        /// Length of the value we tried to write.
        size: usize,
        /// Capacity of the destination section.
        capacity: usize,
    },

    /// `mkdwarfs` exited with a non-zero status.
    #[error("mkdwarfs failed: {0}\n  hint: check that the AppDir contents are valid and readable")]
    DwarfsFailed(String),

    /// `mksquashfs` exited with a non-zero status.
    #[error("mksquashfs failed: {0}")]
    SquashfsFailed(String),

    /// Failed to build the metadata SquashFS sidecar.
    #[error("failed to build metadata SquashFS sidecar: {0}")]
    MetadataSidecar(String),

    /// The metadata SquashFS is larger than its fixed slot.
    #[error(
        "metadata SquashFS is {size} bytes, exceeding the fixed {limit}-byte sidecar slot\n  \
         hint: use a 256x256 PNG .DirIcon or rebuild the runtime with a larger metadata scan limit"
    )]
    MetadataSidecarTooLarge {
        /// Size of the built metadata image.
        size: usize,
        /// Fixed slot size available for the sidecar.
        limit: usize,
    },

    /// Failed to generate the `.zsync` control file.
    #[error(
        "zsync generation failed: {0}\n  \
         hint: ensure the output AppImage was created successfully"
    )]
    ZsyncGenerate(String),

    /// Failed to write the generated `.zsync` control file to disk.
    #[error("zsync write failed: {0}")]
    ZsyncWrite(String),

    /// Configuration or precondition error not covered by a more specific variant.
    #[error("{0}")]
    Config(String),

    /// Wraps an underlying I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<zsync_rs::GenerateError> for Error {
    fn from(e: zsync_rs::GenerateError) -> Self {
        Error::ZsyncGenerate(e.to_string())
    }
}

impl From<zsync_rs::WriteError> for Error {
    fn from(e: zsync_rs::WriteError) -> Self {
        Error::ZsyncWrite(e.to_string())
    }
}

/// `Result` alias for crate operations.
pub type Result<T> = std::result::Result<T, Error>;
