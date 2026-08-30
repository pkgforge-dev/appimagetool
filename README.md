# appimagetool

[![GitHub Downloads](https://img.shields.io/github/downloads/pkgforge-dev/appimagetool/total?logo=github&label=GitHub%20Downloads)](https://github.com/pkgforge-dev/appimagetool/releases/latest)
[![CI Build Status](https://github.com//pkgforge-dev/appimagetool/actions/workflows/release.yml/badge.svg)](https://github.com/pkgforge-dev/appimagetool/releases/latest)
[![Latest Stable Release](https://img.shields.io/github/v/release/pkgforge-dev/appimagetool)](https://github.com/pkgforge-dev/appimagetool/releases/latest)

A Rust implementation of `appimagetool` for the [Anylinux-AppImages](https://github.com/pkgforge-dev/Anylinux-AppImages) project.

It takes a prepared AppDir and produces a finished `.AppImage` using DWARFS compression and the [uruntime](https://github.com/pkgforge-dev/Anylinux-uruntime) AppImage runtime. Single binary, no Python, no C++ deps.

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

### Running tests

```sh
# Unit + integration tests
cargo test --all-features

# Full end-to-end test (requires mkdwarfs + uruntime installed)
cargo test --all-features -- --ignored
```

## Project layout

```
src/
├── main.rs       CLI entry point (clap)
├── lib.rs        library root
├── appimage.rs   build pipeline orchestration
├── config.rs     CLI args + env var resolution
├── desktop.rs    .desktop parsing and metadata
├── dwarfs.rs     mkdwarfs resolution, image building, profiling
├── elf.rs        ELF section read / write / patch
├── error.rs      error types with actionable hints
├── log.rs        verbosity-gated logger
├── uruntime.rs   runtime download, caching, configuration
└── util.rs       atomic downloads, sanitization, ELF detection
```
