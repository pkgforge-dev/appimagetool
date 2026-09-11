//! Library backing the `appimagetool` CLI. Builds AppImages from an AppDir:
//! resolves a uruntime, patches its ELF sections, builds a DWARFS image, and
//! emits the final AppImage plus optional zsync control file.
//!
//! Most callers want [`appimage::build`] with a [`config::Config`].

#![deny(missing_docs)]

pub mod appimage;
pub mod config;
pub mod desktop;
pub mod dwarfs;
pub mod elf;
pub mod embed;
pub mod error;
pub mod log;
/// Pinned upstream artifact URLs and SHA-256 digests, shared with `build.rs`.
pub mod pinned;
pub mod uruntime;
pub mod util;
