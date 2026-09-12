# appimagetool

[![GitHub Downloads](https://img.shields.io/github/downloads/pkgforge-dev/appimagetool/total?logo=github&label=GitHub%20Downloads)](https://github.com/pkgforge-dev/appimagetool/releases/latest)
[![CI Build Status](https://github.com//pkgforge-dev/appimagetool/actions/workflows/release.yml/badge.svg)](https://github.com/pkgforge-dev/appimagetool/releases/latest)
[![Latest Stable Release](https://img.shields.io/github/v/release/pkgforge-dev/appimagetool)](https://github.com/pkgforge-dev/appimagetool/releases/latest)

A Rust implementation of `appimagetool` for the [Anylinux-AppImages](https://github.com/pkgforge-dev/Anylinux-AppImages) project.

It takes a prepared AppDir and produces a finished `.AppImage` using DWARFS compression and the [uruntime](https://github.com/VHSgunzo/uruntime) AppImage runtime. Single binary, no Python, no C++ deps.

## Quick start

```sh
appimagetool ./AppDir
```

That's it. The tool will:

1. Validate the AppDir (checks for `AppRun`, `.DirIcon`, one `.desktop` file).
2. Download `mkdwarfs` and `uruntime` if they're not already cached.
3. Write `X-AppImage-*` metadata into the desktop entry.
4. Build the DWARFS image with the runtime embedded as the ELF header.
5. Generate a `.zsync` file when update info is set, plus an `appinfo` sidecar.

## Features

- DWARFS compression for small, delta-friendly AppImages.
- Drop-in uruntime integration with automatic download and ELF section patching.
- Optional profile-guided optimization (DWARFS hotness profiling) for faster launches.
- Built-in zsync generation, no `zsyncmake` shell-out.
- No central repository, no daemon, no extra runtime deps.

## Usage

### Typical build

```sh
appimagetool ./build/AppDir \
  --output ./dist \
  --update-info "gh-releases-zsync|org|repo|latest|*-x86_64.AppImage.zsync"
```

### CI / GitHub Actions

If `GITHUB_REPOSITORY` is set, update info is auto-derived as
`gh-releases-zsync|owner|repo|latest|*<arch>.AppImage.zsync`. So in CI you usually just need:

```sh
VERSION=1.2.3 appimagetool ./AppDir --output ./dist
```

### Cross-arch / aliased filenames

The tool tracks two arches separately:

- `APPIMAGE_ARCH` is the *runtime* arch. It picks which `mkdwarfs` and `uruntime` binaries to download, and shows up in `X-AppImage-Arch` metadata. Defaults to the host arch.
- `ARCH` is the *display* arch. It only affects the output filename. Falls back to `APPIMAGE_ARCH` when unset.

This lets a project publish under an alias like `amd64` while still pulling the correct `x86_64` runtime:

```sh
APPIMAGE_ARCH=x86_64 ARCH=amd64 appimagetool ./AppDir
# produces MyApp-1.2.3-anylinux-amd64.AppImage with X-AppImage-Arch=x86_64
```

### All CLI options

```
Usage: appimagetool [OPTIONS] [APPDIR]

Arguments:
  [APPDIR]  Path to the AppDir directory [env: APPDIR] [default: ./AppDir]

Options:
  -o, --output <OUTPUT>            Output directory [env: OUTPATH] [default: .]
  -n, --name <NAME>                Output filename (auto-detected from .desktop) [env: OUTNAME]
      --appimage-arch <ARCH>       Runtime arch (download URL + X-AppImage-Arch) [env: APPIMAGE_ARCH]
      --arch <ARCH>                Display arch used in the output filename [env: ARCH]
      --runtime <RUNTIME>          Path to uruntime binary [env: RUNTIME]
      --runtime-url <URL>          URL to download uruntime from [env: URUNTIME_LINK]
  -u, --update-info <UPINFO>       Update information string [env: UPINFO]
      --dwarfs-comp <COMP>         DWARFS compression options [env: DWARFS_COMP]
      --optimize-launch            Enable DWARFS profile optimization (also via OPTIMIZE_LAUNCH=1)
      --profile-timeout <SECS>     Profiling timeout in seconds [env: OPTIMIZE_LAUNCH_TIMEOUT] [default: 10]
      --keep-mount                 Keep the FUSE mount alive after exit (also via URUNTIME_PRELOAD=1)
      --devel-release              Tag the build as a nightly/devel release (also via DEVEL_RELEASE=1)
      --dwarfs-profile <PROFILE>   Path to DWARFS profile [env: DWARFSPROF]
      --mkdwarfs <PATH>            Path to mkdwarfs binary [env: DWARFS_CMD]
      --dwarfs-url <URL>           URL to download mkdwarfs from [env: DWARFS_LINK]
      --tmpdir <TMPDIR>            Temporary directory [env: TMPDIR] [default: /tmp]
      --license                    Print license and third-party notices for this build
  -v, --verbose...                 Increase verbosity (-v, -vv)
  -q, --quiet...                   Suppress informational output (-q, -qq)
  -h, --help                       Print help
  -V, --version                    Print version
```

### Environment variables

Every CLI option has a matching env var (shown above). A few extra knobs that are env-only:

| Variable             | Effect                                                                                  |
| -------------------- | --------------------------------------------------------------------------------------- |
| `VERSION`            | Version string baked into the AppImage filename and metadata.                           |
| `APPNAME`            | Override the application name (defaults to the desktop entry's `Name`).                 |
| `GITHUB_REPOSITORY`  | When set, auto-generates `update_info` as `gh-releases-zsync\|owner\|repo\|latest\|...` |
| `ADD_PERMA_ENV_VARS` | Permanent env vars to bake into the runtime, one per line.                              |
| `URUNTIME_PRELOAD`   | Set to `1` to keep the FUSE mount after the app exits.                                  |
| `DEVEL_RELEASE`      | Set to `1` to tag the build as a nightly/devel release.                                 |
| `OPTIMIZE_LAUNCH`    | Set to `1` to enable the DWARFS profiling pass (same as `--optimize-launch`).           |
| `OPTIMIZE_LAUNCH_TIMEOUT` | Profiling timeout in seconds (default `10`).                                       |
| `SKIP_INTEGRITY_CHECKS` | Set to `1` to skip the pinned uruntime SHA-256 verification.                         |

### AppDir requirements

The AppDir must contain:

1. An executable `AppRun`.
2. A `.DirIcon` file (PNG or SVG).
3. Exactly one `.desktop` file in the AppDir root.

### Output filename

By default:

```
<AppName>-<Version>-anylinux-<Arch>.AppImage
```

Where `AppName` comes from the `.desktop` `Name` key (or `APPNAME`), `Version` from `VERSION` (or `~/version`), and `Arch` is the display arch (`ARCH`, falling back to `APPIMAGE_ARCH`).

You can also pin the filename outright with `--name` / `OUTNAME`.

## Building

```sh
cargo build --release
```

### Embedding the helper binaries

By default appimagetool downloads its helpers on demand: the pinned uruntime
always, and `mkdwarfs` only when it is not already on `$PATH`. Two optional
features bake them into the binary instead, so a build needs no network access
at AppImage build time.

```sh
# Embed the pinned uruntime. Stays MIT.
cargo build --release --features embed-uruntime

# Also embed mkdwarfs. See the licensing note below before using this.
cargo build --release --features embed-uruntime,embed-mkdwarfs
```

Release artifacts ship in two variants. `appimagetool-<arch>` is built with no
features: it is MIT and downloads its helpers on demand. `appimagetool-full-<arch>`
is built with both features, so it needs no network at all for helper
resolution — which makes it GPL-3.0-or-later. See [Licensing](#licensing).

Embedded blobs are architecture specific. A build that embeds a helper for its
own target still falls back to downloading when asked for a different
`--appimage-arch`, and an explicit `--runtime` / `--mkdwarfs` path or a custom
`--runtime-url` / `--dwarfs-url` always wins over an embedded copy.

#### Building offline

`build.rs` resolves each blob in this order, verifying the pinned SHA-256 at
every step:

1. an explicit path in `URUNTIME_EMBED_PATH` / `MKDWARFS_EMBED_PATH`, or a file
   named `{uruntime,mkdwarfs}-{arch}` inside `APPIMAGETOOL_EMBED_DIR`
2. a digest-valid file in the in-tree `.embed-cache/` directory
3. a download from the pinned upstream URL

Packagers building in a network-isolated sandbox should pre-fetch the artifact
and use step 1, which never reaches the network:

```sh
URUNTIME_EMBED_PATH=/path/to/uruntime-x86_64 \
  cargo build --release --features embed-uruntime
```

A default build runs no network requests in `build.rs` at all, so it works
unchanged on docs.rs and in distro build sandboxes.

### Licensing

appimagetool's own source is MIT in every configuration. The `embed-mkdwarfs`
feature changes the license of the resulting **binary**, because DwarFS is split
licensed: the code that reads images is MIT, while the `mkdwarfs` writer is
GPL-3.0.

| Build | Embedded | Binary license |
| --- | --- | --- |
| default | nothing | MIT |
| `embed-uruntime` | uruntime + DwarFS reader | MIT |
| `embed-mkdwarfs` | `dwarfs-universal` | GPL-3.0-or-later |

`embed-uruntime` is MIT throughout because it pins the `-lite` uruntime variant,
which carries only DwarFS `dwarfs-fuse-extract` (the MIT reader and extractor)
rather than `dwarfs-universal`.

**The `appimagetool-full-<arch>` artifacts published on the releases page are
GPL-3.0-or-later**, because they are built with `embed-mkdwarfs`. The plain
`appimagetool-<arch>` artifacts are MIT and download their helpers on demand.
Each release therefore also ships the DwarFS Corresponding Source as
`appimagetool-corresponding-source-dwarfs-<version>.tar.xz`, and the
`appimagetool-full-<arch>` tarballs include `licenses/`, which holds the full
GPL-3.0 text along with the MIT notices for uruntime and the DwarFS reader.

If you need an MIT-licensed binary, use the plain `appimagetool-<arch>`
artifact or build from source without `embed-mkdwarfs`. The crate source itself
is MIT in every configuration, so `cargo build --release` or
`--features embed-uruntime` both give you an MIT artifact.

Anyone redistributing a GPL-3.0 build is conveying a GPL-3.0 work and must keep
that Corresponding Source available to recipients for as long as the binary is
offered. See `licenses/mkdwarfs-GPL-3.0.txt` for the details.

`appimagetool --license` reports the effective license of any build along with
the notices for whatever that build actually embeds.

### Running tests

```sh
# Unit + integration tests
cargo test --all-features

# Full end-to-end test (requires mkdwarfs + uruntime installed)
cargo test --all-features -- --ignored
```

## Project layout

```
build.rs          resolves the optional embedded helper blobs
licenses/         notices shipped with builds that embed a helper
src/
├── main.rs       CLI entry point (clap)
├── lib.rs        library root
├── appimage.rs   build pipeline orchestration
├── config.rs     CLI args + env var resolution
├── desktop.rs    .desktop parsing and metadata
├── dwarfs.rs     mkdwarfs resolution, image building, profiling
├── elf.rs        ELF section read / write / patch
├── embed.rs      embedded helper access + license reporting
├── error.rs      error types with actionable hints
├── log.rs        verbosity-gated logger
├── pinned.rs     pinned upstream URLs + SHA-256 digests (shared with build.rs)
├── uruntime.rs   runtime download, caching, configuration
└── util.rs       atomic downloads, sanitization, ELF detection
```
