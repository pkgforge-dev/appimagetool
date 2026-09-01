//! Resolved build configuration plus the [`CliArgs`] input that produces it.

use std::env;
use std::path::PathBuf;

fn env_opt(name: &str) -> Option<String> {
    env::var(name).ok()
}

fn env_truthy(name: &str) -> bool {
    matches!(
        env_opt(name).as_deref().map(str::trim),
        Some("1" | "true" | "TRUE" | "True" | "yes" | "YES" | "Yes" | "on" | "ON" | "On")
    )
}

/// Fully resolved build configuration, produced by [`Config::from_cli_args`].
/// CLI flags, env-var fallbacks, and built-in defaults are all merged here, so
/// downstream code can treat this struct as authoritative.
#[derive(Debug)]
pub struct Config {
    /// Directory holding the AppDir contents.
    pub appdir: PathBuf,
    /// Directory the final `.AppImage` (and zsync) will be written to.
    pub output_dir: PathBuf,
    /// Override for the output filename; otherwise derived from desktop entry.
    pub output_name: Option<String>,
    /// Runtime architecture used for downloading uruntime/mkdwarfs and for
    /// the `X-AppImage-Arch` metadata. Defaults to the host arch (`uname -m`).
    pub appimage_arch: String,
    /// Display architecture used in the output filename (e.g. `amd64`).
    /// Falls back to [`Self::appimage_arch`] when not explicitly set —
    /// some projects publish artifacts under aliases like `amd64`/`arm64`.
    pub arch: String,
    /// Explicit uruntime binary to embed; resolved/downloaded if `None`.
    pub runtime: Option<PathBuf>,
    /// Override URL for downloading the uruntime.
    pub runtime_url: Option<String>,
    /// Compression options passed to `mkdwarfs`.
    pub dwarfs_comp: String,
    /// `upd_info` ELF section payload (zsync URL or similar).
    pub update_info: Option<String>,
    /// Permanent env-var lines to bake into the runtime's `.envs` section.
    pub env_vars: Vec<String>,
    /// Existing DWARFS profile to seed the build with.
    pub dwarfs_profile: Option<PathBuf>,
    /// If true, run a profiling pass before the final build.
    pub optimize_launch: bool,
    /// Resolved package version, if discovered from `VERSION` or `~/version`.
    pub version: Option<String>,
    /// Override `mkdwarfs` binary path; resolved from PATH/cache otherwise.
    pub mkdwarfs: Option<PathBuf>,
    /// Override URL for downloading `mkdwarfs`.
    pub dwarfs_url: Option<String>,
    /// Temp directory used for caches, working copies, and FUSE mounts.
    pub tmpdir: PathBuf,
    /// Patch the runtime to keep the FUSE mount alive after exit.
    pub keep_mount: bool,
    /// Tag the build as a nightly/devel release (rewrites desktop name + zsync).
    pub devel_release: bool,
    /// How long, in seconds, to let the AppImage run during the DWARFS
    /// profiling pass before tearing it down.
    pub profile_timeout: u64,
}

/// Inputs gathered from the CLI (or set explicitly by library callers).
/// `Default` produces `None`/`false` for every field, so callers can use
/// `..Default::default()` to fill in whatever they don't care about — the
/// final `Config` still applies all env-var fallbacks and defaults.
#[derive(Debug, Default, Clone)]
pub struct CliArgs {
    /// AppDir path; defaults to `./AppDir`.
    pub appdir: Option<PathBuf>,
    /// Output directory; defaults to `.`.
    pub output: Option<PathBuf>,
    /// Override for the output filename.
    pub output_name: Option<String>,
    /// Runtime architecture (download URL + `X-AppImage-Arch` metadata).
    /// Defaults to `APPIMAGE_ARCH` env / host arch.
    pub appimage_arch: Option<String>,
    /// Display architecture used in the output filename.
    /// Defaults to [`Self::appimage_arch`] — set this to publish under an
    /// alias like `amd64` while keeping the runtime arch as `x86_64`.
    pub arch: Option<String>,
    /// Path to a pre-supplied uruntime binary.
    pub runtime: Option<PathBuf>,
    /// URL for downloading the uruntime.
    pub runtime_url: Option<String>,
    /// `upd_info` payload override (e.g. a zsync URL).
    pub update_info: Option<String>,
    /// `mkdwarfs` compression option string.
    pub dwarfs_comp: Option<String>,
    /// Enable the DWARFS profiling pass.
    pub optimize_launch: bool,
    /// Path to an existing DWARFS profile to feed into the build.
    pub dwarfs_profile: Option<PathBuf>,
    /// Path to a pre-supplied `mkdwarfs` binary.
    pub mkdwarfs: Option<PathBuf>,
    /// URL for downloading `mkdwarfs`.
    pub dwarfs_url: Option<String>,
    /// Override TMPDIR.
    pub tmpdir: Option<PathBuf>,
    /// Patch the runtime to keep the FUSE mount alive after exit.
    /// Also enabled by `URUNTIME_PRELOAD=1`.
    pub keep_mount: bool,
    /// Tag the build as a nightly/devel release.
    /// Also enabled by `DEVEL_RELEASE=1`.
    pub devel_release: bool,
    /// Override the DWARFS profiling timeout (seconds). Defaults to `10`.
    pub profile_timeout: Option<u64>,
}

impl Config {
    /// Resolve a fully-defaulted [`Config`] from CLI inputs and env vars.
    pub fn from_cli_args(args: CliArgs) -> crate::error::Result<Self> {
        // Env vars with clap `env =` are already resolved by the CLI.
        // The .or_else() calls below handle the library case (no clap).

        let appimage_arch = args
            .appimage_arch
            .or_else(|| env_opt("APPIMAGE_ARCH"))
            .unwrap_or_else(|| env::consts::ARCH.to_string());
        // Normalize rust arch spellings to the uname -m equivalent so the
        // download URLs resolve (e.g. powerpc64le -> ppc64le).
        let appimage_arch = match appimage_arch.as_str() {
            "powerpc64" => "ppc64".to_string(),
            "powerpc64le" => "ppc64le".to_string(),
            other => other.to_string(),
        };
        let arch = args.arch.unwrap_or_else(|| appimage_arch.clone());

        let appdir = args.appdir.unwrap_or_else(|| PathBuf::from("./AppDir"));
        let tmpdir = args.tmpdir.unwrap_or_else(|| PathBuf::from("/tmp"));

        let optimize_launch = args.optimize_launch || env_truthy("OPTIMIZE_LAUNCH");

        let keep_mount = args.keep_mount || env_truthy("URUNTIME_PRELOAD");
        let devel_release = args.devel_release || env_truthy("DEVEL_RELEASE");

        let profile_timeout = args
            .profile_timeout
            .or_else(|| env_opt("OPTIMIZE_LAUNCH_TIMEOUT").and_then(|s| s.parse().ok()))
            .unwrap_or(10);

        let update_info = args.update_info.or_else(|| env_opt("UPINFO")).or_else(|| {
            env::var("GITHUB_REPOSITORY").ok().map(|repo| {
                let parts: Vec<&str> = repo.split('/').collect();
                if parts.len() == 2 {
                    format!(
                        "gh-releases-zsync|{}|{}|latest|*{arch}.AppImage.zsync",
                        parts[0], parts[1]
                    )
                } else {
                    repo
                }
            })
        });

        let env_vars = env_opt("ADD_PERMA_ENV_VARS")
            .map(|s| s.lines().map(|l| l.to_string()).collect())
            .unwrap_or_default();

        let dwarfs_profile = args
            .dwarfs_profile
            .or_else(|| env_opt("DWARFSPROF").map(PathBuf::from))
            .or_else(|| optimize_launch.then(|| appdir.join(".dwarfsprofile")));

        let dwarfs_comp = args
            .dwarfs_comp
            .unwrap_or_else(|| "zstd:level=19 -S26 -B6".to_string());

        Ok(Config {
            appdir,
            output_dir: args.output.unwrap_or_else(|| PathBuf::from(".")),
            output_name: args.output_name,
            appimage_arch,
            arch,
            runtime: args.runtime,
            runtime_url: args.runtime_url,
            dwarfs_comp,
            update_info,
            env_vars,
            dwarfs_profile,
            optimize_launch,
            version: resolve_version(),
            mkdwarfs: args.mkdwarfs,
            dwarfs_url: args.dwarfs_url.or_else(|| env_opt("DWARFS_LINK")),
            tmpdir,
            keep_mount,
            devel_release,
            profile_timeout,
        })
    }
}

/// Resolve the project version from the `VERSION` env var, falling back to
/// `~/version` if present. Strips an optional `epoch:` prefix.
fn resolve_version() -> Option<String> {
    let raw = env_opt("VERSION").or_else(|| {
        let v = dirs_home().join("version");
        v.exists()
            .then(|| {
                std::fs::read_to_string(&v)
                    .ok()
                    .map(|s| s.trim().to_string())
            })
            .flatten()
    })?;
    Some(strip_epoch(&raw))
}

fn dirs_home() -> PathBuf {
    env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("~"))
}

/// Strip the epoch prefix from a version string.
/// Shell equivalent: `${VERSION#*:}` — removes everything up to
/// and including the first colon.  `"1:2.0.1"` → `"2.0.1"`.
fn strip_epoch(version: &str) -> String {
    match version.find(':') {
        Some(i) => version[i + 1..].to_string(),
        None => version.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_epoch() {
        assert_eq!(strip_epoch("1:2.0.1"), "2.0.1");
        assert_eq!(strip_epoch("2.0.1"), "2.0.1");
        assert_eq!(strip_epoch("1:"), "");
        assert_eq!(strip_epoch(""), "");
        assert_eq!(strip_epoch("0:1.0.0-alpha"), "1.0.0-alpha");
    }

    fn args_with_appdir() -> CliArgs {
        CliArgs {
            appdir: Some(PathBuf::from("/tmp/AppDir")),
            ..Default::default()
        }
    }

    #[test]
    fn test_config_custom_compression() {
        let config = Config::from_cli_args(CliArgs {
            dwarfs_comp: Some("zstd:level=1".to_string()),
            ..args_with_appdir()
        })
        .unwrap();
        assert_eq!(config.dwarfs_comp, "zstd:level=1");
    }

    #[test]
    fn test_config_update_info_passthrough() {
        let config = Config::from_cli_args(CliArgs {
            update_info: Some("gh-releases-zsync|org|repo|latest|*.AppImage.zsync".to_string()),
            ..args_with_appdir()
        })
        .unwrap();
        assert_eq!(
            config.update_info.as_deref(),
            Some("gh-releases-zsync|org|repo|latest|*.AppImage.zsync")
        );
    }

    #[test]
    fn test_config_output_name_override() {
        let config = Config::from_cli_args(CliArgs {
            output_name: Some("custom.AppImage".to_string()),
            ..args_with_appdir()
        })
        .unwrap();
        assert_eq!(config.output_name.as_deref(), Some("custom.AppImage"));
    }

    #[test]
    fn test_config_runtime_path() {
        let config = Config::from_cli_args(CliArgs {
            runtime: Some(PathBuf::from("/opt/uruntime")),
            ..args_with_appdir()
        })
        .unwrap();
        assert_eq!(
            config.runtime.as_deref(),
            Some(std::path::Path::new("/opt/uruntime"))
        );
    }

    #[test]
    fn test_config_env_vars_from_string() {
        let vars: Vec<String> = "FOO=bar\nBAZ=qux".lines().map(|l| l.to_string()).collect();
        assert_eq!(vars, vec!["FOO=bar", "BAZ=qux"]);
    }

    #[test]
    fn test_dirs_home_returns_something() {
        let home = dirs_home();
        assert!(!home.as_os_str().is_empty());
    }
}
