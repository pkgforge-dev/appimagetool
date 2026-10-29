//! Build DWARFS filesystem images and run the optional profiling pass that
//! feeds `--categorize=hotness` into the final image.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::embed;
use crate::error::{Error, Result};
use crate::pinned;
use crate::util;

/// Compression algorithm of the default `mkdwarfs` options. The block size
/// (`-S`) and [`DEFAULT_BLOCK_OPTS`] are appended by [`default_compression`].
const DEFAULT_ALGO: &str = "zstd:level=22";
/// `-B2` keeps the last two blocks in the segmenter's lookback window.
const DEFAULT_BLOCK_OPTS: &str = "-B2";

/// mkdwarfs compresses blocks in parallel, one block per worker. The default
/// block size is the smallest one that keeps the block count at or below this
/// value, so a small AppDir is split into up to four blocks (four vCPUs on the
/// hosted CI runners) while a large one keeps few, large blocks for the ratio.
const TARGET_BLOCKS: u64 = 4;
/// Smallest default block size, `-S24` (16 MiB). Below this, extra blocks cost
/// ratio and a small AppDir gains nothing.
const MIN_BLOCK_BITS: u32 = 24;
/// Largest default block size, `-S26` (64 MiB): the best ratio, and the value
/// this tool used for every AppDir before the size was considered.
const MAX_BLOCK_BITS: u32 = 26;

/// Build the default `mkdwarfs` compression option string for an AppDir.
///
/// The `-S` value comes from the size of the AppDir at the time of the call,
/// see [`block_bits_for_size`]. If the size cannot be read, `-S26` is used.
pub fn default_compression(appdir: &Path) -> String {
    let size = appdir_size(appdir);
    let bits = match size {
        Some(size) => block_bits_for_size(size),
        None => MAX_BLOCK_BITS,
    };
    crate::log_debug!(
        "AppDir {} holds {} bytes of file data, using -S{bits}",
        appdir.display(),
        size.map_or_else(|| "an unknown number of".to_string(), |s| s.to_string())
    );
    format!("{DEFAULT_ALGO} -S{bits} {DEFAULT_BLOCK_OPTS}")
}

/// The `-S` value for `size` bytes of file data: the smallest block size that
/// gives at most [`TARGET_BLOCKS`] blocks, clamped to `-S24..=-S26`. A size
/// that is an exact power-of-two multiple of the target (64 MiB, 128 MiB)
/// takes the smaller block.
///
/// Above 64 MiB the block count steps down just past each boundary, for
/// example from 4 blocks at 64 MiB to 3 at 64.5 MiB, because block sizes are
/// powers of two and the count is capped.
fn block_bits_for_size(size: u64) -> u32 {
    size.div_ceil(TARGET_BLOCKS)
        .max(1)
        .next_power_of_two()
        .trailing_zeros()
        .clamp(MIN_BLOCK_BITS, MAX_BLOCK_BITS)
}

/// Total size in bytes of the regular files under `dir`, without following
/// symlinks. Returns `None` if the directory or any entry cannot be read.
///
/// For a finished AppDir this overstates the data mkdwarfs stores (hard links
/// are counted once per path, and mkdwarfs deduplicates identical content), so
/// it does not pick a block size larger than the data warrants. Symlinks and
/// files added after the walk are the only sources of understatement.
fn appdir_size(dir: &Path) -> Option<u64> {
    let mut total: u64 = 0;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in std::fs::read_dir(&current).ok()? {
            let entry = entry.ok()?;
            let file_type = entry.file_type().ok()?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                total = total.saturating_add(entry.metadata().ok()?.len());
            }
        }
    }
    Some(total)
}

/// Resolve the mkdwarfs binary path. Checks the user-provided path, then
/// `$PATH`, then a blob embedded by the `embed-mkdwarfs` feature, then downloads
/// to tmpdir.
///
/// `mkdwarfs` runs on the build host, so the embedded blob only applies when the
/// host and requested architectures match. A cross-arch build falls through to
/// downloading, preserving the existing behaviour of fetching the asset for
/// `config.appimage_arch`.
pub fn resolve_mkdwarfs(config: &Config) -> Result<PathBuf> {
    // User-provided path
    if let Some(ref path) = config.mkdwarfs {
        if path.exists() {
            return Ok(path.clone());
        }
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("mkdwarfs not found at {}", path.display()),
        )));
    }

    // Check $PATH. A system mkdwarfs still wins over an embedded copy: the
    // feature exists to avoid the download, not to override the host toolchain.
    if let Ok(path) = which("mkdwarfs") {
        return Ok(path);
    }

    let cached = config.tmpdir.join("mkdwarfs");

    if config.dwarfs_url.is_none()
        && let Some(bytes) = embed::mkdwarfs(&config.appimage_arch)
    {
        crate::log_info!("Using embedded mkdwarfs for {}...", config.appimage_arch);
        embed::materialize(bytes, &cached)?;
        return Ok(cached);
    }

    // Checksums are only known for the pinned default URL; user-supplied URLs
    // are downloaded without verification.
    let expected_checksum = if config.dwarfs_url.is_none() {
        pinned::checksum_for(pinned::MKDWARFS_CHECKSUMS, &config.appimage_arch)
    } else {
        None
    };
    let url = pinned::url_for(
        config
            .dwarfs_url
            .as_deref()
            .unwrap_or(pinned::MKDWARFS_URL_TEMPLATE),
        &config.appimage_arch,
    );
    util::ensure_cached_binary(&cached, &url, "mkdwarfs", expected_checksum)?;
    Ok(cached)
}

/// Build a DWARFS AppImage. The runtime is embedded via `--header`.
pub fn build_appimage(
    mkdwarfs: &Path,
    appdir: &Path,
    runtime: &Path,
    output: &Path,
    compression: &str,
    profile: Option<&Path>,
) -> Result<()> {
    crate::log_info!("Building DWARFS AppImage...");

    let mut cmd = Command::new(mkdwarfs);
    cmd.arg("--force")
        .arg("--order=path")
        .arg("--set-owner")
        .arg("0")
        .arg("--set-group")
        .arg("0")
        .arg("--no-history")
        .arg("--no-create-timestamp")
        .arg("--header")
        .arg(runtime)
        .arg("--input")
        .arg(appdir);

    // Add profile optimization if available
    if let Some(profile) = profile
        && profile.exists()
    {
        crate::log_info!("Using DWARFS profile {}...", profile.display());
        cmd.arg("--categorize=hotness")
            .arg(format!("--hotness-list={}", profile.display()));
    }

    // Add compression options. The string can contain multiple space-separated
    // args like "zstd:level=22 -S26 -B2". The first part goes to -C, the rest
    // are passed as separate args.
    let mut parts = compression.split_whitespace().peekable();
    if let Some(comp_algo) = parts.next() {
        cmd.arg("-C").arg(comp_algo);
    }
    for part in parts {
        cmd.arg(part);
    }

    cmd.arg("--output").arg(output);

    let status = cmd.status()?;
    if !status.success() {
        return Err(Error::DwarfsFailed(format!(
            "mkdwarfs exited with status {}",
            status.code().unwrap_or(-1)
        )));
    }

    Ok(())
}

/// Build a temporary AppImage for DWARFS profiling with basic compression.
pub fn build_profile_image(
    mkdwarfs: &Path,
    appdir: &Path,
    runtime: &Path,
    output: &Path,
) -> Result<()> {
    crate::log_info!("Building temporary image for DWARFS profiling...");

    let status = Command::new(mkdwarfs)
        .arg("--force")
        .arg("--order=path")
        .arg("--set-owner")
        .arg("0")
        .arg("--set-group")
        .arg("0")
        .arg("--no-history")
        .arg("--no-create-timestamp")
        .arg("--header")
        .arg(runtime)
        .arg("--input")
        .arg(appdir)
        .arg("-C")
        .arg("zstd:level=5")
        .arg("-S19")
        .arg("--output")
        .arg(output)
        .status()?;

    if !status.success() {
        return Err(Error::DwarfsFailed(format!(
            "mkdwarfs (profile build) exited with status {}",
            status.code().unwrap_or(-1)
        )));
    }

    Ok(())
}

/// Verify FUSE is usable in this environment. AppImage launch requires `/dev/fuse`
/// to be present and accessible (commonly missing in minimal containers).
pub fn check_fuse_available() -> Result<()> {
    let dev_fuse = Path::new("/dev/fuse");
    if !dev_fuse.exists() {
        return Err(Error::Config(
            "FUSE is not available: /dev/fuse is missing\n  \
             hint: OPTIMIZE_LAUNCH launches the AppImage to record a profile, \
             which requires FUSE. Load the `fuse` kernel module, or run the \
             container with `--device /dev/fuse --cap-add SYS_ADMIN`."
                .to_string(),
        ));
    }
    Ok(())
}

/// Run DWARFS profiling: launch a temp AppImage under xvfb with DWARFS_ANALYSIS_FILE set,
/// wait for a timeout, then tear down the process tree and unmount only the FUSE
/// mounts that we created.
pub fn run_profiling(
    appimage: &Path,
    profile_output: &Path,
    tmpdir: &Path,
    timeout_secs: u64,
) -> Result<()> {
    use std::os::unix::process::CommandExt;

    let tmp_profile = util::process_unique_path(tmpdir, "dwarfsprof.tmp");

    // Snapshot pre-existing AppImage FUSE mounts so we don't disturb other
    // running AppImages on the system when we clean up.
    let pre_mounts = snapshot_appimage_mounts(tmpdir);

    crate::log_info!("Running DWARFS profiling for {timeout_secs}s...");

    let mut child = Command::new("xvfb-run")
        .arg("-a")
        .arg("--")
        .arg(appimage)
        .env("TMPDIR", tmpdir)
        .env("DWARFS_ANALYSIS_FILE", &tmp_profile)
        // Create a new process group so we can kill the tree
        .process_group(0)
        .spawn()
        .map_err(|e| Error::Config(format!("failed to spawn xvfb-run: {e}")))?;

    let pgid = child.id() as i32;

    std::thread::sleep(Duration::from_secs(timeout_secs));

    terminate_process_group(&mut child, pgid);

    // Unmount only the mounts that appeared during this run.
    let post_mounts = snapshot_appimage_mounts(tmpdir);
    for mountpoint in post_mounts.difference(&pre_mounts) {
        unmount(mountpoint);
    }

    // Give the profile writer a moment to finalize
    std::thread::sleep(Duration::from_secs(2));

    if tmp_profile.exists() {
        std::fs::copy(&tmp_profile, profile_output)?;
        let _ = std::fs::remove_file(&tmp_profile);
        crate::log_info!("DWARFS profile written to {}", profile_output.display());
    } else {
        crate::log_warn!("DWARFS profile was not generated");
    }

    let _ = std::fs::remove_file(appimage);

    Ok(())
}

/// Send SIGTERM to the process group, wait briefly for graceful exit, then
/// escalate to SIGKILL if the tree refuses to die. Always returns; never blocks
/// indefinitely on a stuck child.
fn terminate_process_group(child: &mut std::process::Child, pgid: i32) {
    unsafe {
        libc::kill(-pgid, libc::SIGTERM);
    }
    if wait_with_timeout(child, Duration::from_secs(3)) {
        return;
    }

    crate::log_warn!("process did not respond to SIGTERM, sending SIGKILL");
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
    }
    if !wait_with_timeout(child, Duration::from_secs(2)) {
        crate::log_warn!("process did not exit after SIGKILL; continuing anyway");
    }
}

/// Poll `try_wait` until the child exits or the timeout elapses. Returns true
/// if the child exited.
fn wait_with_timeout(child: &mut std::process::Child, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => return true,
        }
    }
    false
}

/// Collect the set of AppImage FUSE mountpoint *directories* (`.mount_*`)
/// currently present in `dir`. The runtime also drops a `.mount_<rand>.pid`
/// sidecar file next to each mount; we deliberately skip it so we don't
/// later call `umount` on a regular file.
fn snapshot_appimage_mounts(dir: &Path) -> HashSet<PathBuf> {
    let mut set = HashSet::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return set;
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(".mount_") {
            continue;
        }
        // Only directories are real mountpoints; .pid sidecars are regular files.
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            set.insert(entry.path());
        }
    }
    set
}

/// Best-effort unmount: try `fusermount -u`, fall back to `umount`, finally
/// `umount -l` (lazy) so a busy mount doesn't block cleanup.
fn unmount(mountpoint: &Path) {
    let try_cmd = |program: &str, args: &[&str]| -> bool {
        Command::new(program)
            .args(args)
            .arg(mountpoint)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    if try_cmd("fusermount", &["-u"]) {
        return;
    }
    if try_cmd("umount", &[]) {
        return;
    }
    let _ = try_cmd("umount", &["-l"]);
}

/// Simple `which` implementation. Only matches a candidate if it's actually
/// executable — a non-`+x` file shadowing the binary on PATH would otherwise
/// be returned and then fail at `Command::status` with a confusing error.
fn which(name: &str) -> std::result::Result<PathBuf, ()> {
    use std::os::unix::fs::PermissionsExt;
    let path_var = std::env::var("PATH").unwrap_or_default();
    for dir in path_var.split(':').filter(|d| !d.is_empty()) {
        let candidate = PathBuf::from(dir).join(name);
        let Ok(meta) = std::fs::metadata(&candidate) else {
            continue;
        };
        if meta.is_file() && meta.permissions().mode() & 0o111 != 0 {
            return Ok(candidate);
        }
    }
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    #[test]
    fn test_block_bits_for_size() {
        let cases: [(u64, u32); 12] = [
            (0, 24),
            (1, 24),
            (16 * MIB, 24),
            (54 * MIB, 24),
            (64 * MIB - 1, 24),
            (64 * MIB, 24),
            (64 * MIB + 1, 25),
            (128 * MIB - 1, 25),
            (128 * MIB, 25),
            (128 * MIB + 1, 26),
            (4 * 1024 * MIB, 26),
            (u64::MAX, 26),
        ];
        for (size, bits) in cases {
            assert_eq!(block_bits_for_size(size), bits, "size {size}");
        }
    }

    #[test]
    fn test_default_blocks_never_exceed_target() {
        for size in [
            1,
            20 * MIB,
            48 * MIB,
            63 * MIB,
            64 * MIB,
            100 * MIB,
            128 * MIB,
        ] {
            let block = 1u64 << block_bits_for_size(size);
            assert!(size.div_ceil(block) <= TARGET_BLOCKS, "size {size}");
        }
    }
}
