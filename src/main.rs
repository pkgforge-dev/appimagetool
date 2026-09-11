use std::path::PathBuf;

use clap::Parser;

/// Create AppImages from an AppDir.
#[derive(Parser)]
#[command(name = "appimagetool", version, about)]
struct Cli {
    /// Path to the AppDir directory
    #[arg(env = "APPDIR", default_value = "./AppDir")]
    appdir: Option<PathBuf>,

    /// Output directory
    #[arg(short, long, env = "OUTPATH", default_value = ".")]
    output: Option<PathBuf>,

    /// Output filename (auto-detected from .desktop if not set)
    #[arg(short = 'n', long, env = "OUTNAME")]
    name: Option<String>,

    /// Runtime architecture (download URL + X-AppImage-Arch metadata)
    #[arg(long, env = "APPIMAGE_ARCH")]
    appimage_arch: Option<String>,

    /// Display architecture used in the output filename (e.g. `amd64`).
    /// Falls back to --appimage-arch when unset.
    #[arg(long, env = "ARCH")]
    arch: Option<String>,

    /// Path to uruntime binary
    #[arg(long, env = "RUNTIME")]
    runtime: Option<PathBuf>,

    /// URL to download uruntime from
    #[arg(long, env = "URUNTIME_LINK")]
    runtime_url: Option<String>,

    /// Update information string
    #[arg(short, long, env = "UPINFO")]
    update_info: Option<String>,

    /// DWARFS compression options
    #[arg(long, env = "DWARFS_COMP")]
    dwarfs_comp: Option<String>,

    /// Enable DWARFS profile optimization
    #[arg(long)]
    optimize_launch: bool,

    /// Profiling timeout in seconds (default 10)
    #[arg(long, env = "OPTIMIZE_LAUNCH_TIMEOUT")]
    profile_timeout: Option<u64>,

    /// Patch the runtime so the FUSE mount stays alive after the app exits
    #[arg(long)]
    keep_mount: bool,

    /// Tag the build as a nightly/devel release
    #[arg(long)]
    devel_release: bool,

    /// Path to DWARFS profile
    #[arg(long, env = "DWARFSPROF")]
    dwarfs_profile: Option<PathBuf>,

    /// Path to mkdwarfs binary
    #[arg(long, env = "DWARFS_CMD")]
    mkdwarfs: Option<PathBuf>,

    /// URL to download mkdwarfs from
    #[arg(long, env = "DWARFS_LINK")]
    dwarfs_url: Option<String>,

    /// Temporary directory
    #[arg(long, env = "TMPDIR", default_value = "/tmp")]
    tmpdir: Option<PathBuf>,

    /// Print license and third-party notices for this build, then exit
    #[arg(long)]
    license: bool,

    /// Increase verbosity (can be repeated: -v, -vv)
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Suppress informational output (can be repeated: -q, -qq)
    #[arg(short = 'q', long, action = clap::ArgAction::Count, conflicts_with = "verbose")]
    quiet: u8,
}

fn main() {
    let cli = Cli::parse();

    if cli.license {
        print!("{}", appimagetool::embed::notices());
        return;
    }

    // Initialize logger: verbosity is (verbose count) - (quiet count)
    let verbosity = cli.verbose as i8 - cli.quiet as i8;
    appimagetool::log::init(verbosity);

    let config = appimagetool::config::Config::from_cli_args(appimagetool::config::CliArgs {
        appdir: cli.appdir,
        output: cli.output,
        output_name: cli.name,
        appimage_arch: cli.appimage_arch,
        arch: cli.arch,
        runtime: cli.runtime,
        runtime_url: cli.runtime_url,
        update_info: cli.update_info,
        dwarfs_comp: cli.dwarfs_comp,
        optimize_launch: cli.optimize_launch,
        dwarfs_profile: cli.dwarfs_profile,
        mkdwarfs: cli.mkdwarfs,
        dwarfs_url: cli.dwarfs_url,
        tmpdir: cli.tmpdir,
        keep_mount: cli.keep_mount,
        devel_release: cli.devel_release,
        profile_timeout: cli.profile_timeout,
    })
    .unwrap_or_else(|e| {
        appimagetool::log_error!("{e}");
        std::process::exit(1);
    });

    if let Err(e) = appimagetool::appimage::build(&config) {
        appimagetool::log_error!("{e}");
        std::process::exit(1);
    }
}
