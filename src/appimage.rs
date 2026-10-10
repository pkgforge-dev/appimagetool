//! Top-level AppImage build pipeline: validate the AppDir, resolve runtime
//! and `mkdwarfs`, optionally run the DWARFS profiling pass, and emit the
//! final `.AppImage` plus optional `.zsync` and `appinfo`.

use std::io::Write;
use std::path::Path;

use zsync_rs::ControlFile;

use crate::config::Config;
use crate::desktop::{self, DesktopEntry};
use crate::dwarfs;
use crate::error::Result;
use crate::uruntime;
use crate::util;

/// Run the full AppImage build pipeline.
pub fn build(config: &Config) -> Result<()> {
    // Validate AppDir
    DesktopEntry::check_apprun(&config.appdir)?;
    DesktopEntry::check_dir_icon(&config.appdir)?;

    // Sort .env: move all "unset" lines to the end.
    // The dotenv library used by sharun requires unset lines last.
    sort_env_file(&config.appdir)?;

    // Parse desktop entry
    let desktop = DesktopEntry::from_appdir(&config.appdir)?;

    // Determine app name: APPNAME env var overrides desktop entry.
    // Sanitize the same way the shell script does.
    let app_name_raw = std::env::var("APPNAME")
        .ok()
        .unwrap_or_else(|| desktop.name.clone());
    let app_name = util::sanitize_filename(&app_name_raw);
    let version = config.version.as_deref().unwrap_or("UNKNOWN");

    // Handle devel release: patch desktop entry and update info
    let update_info = if config.devel_release {
        let content = std::fs::read_to_string(&desktop.path)?;
        if !content.contains("Nightly") {
            let patched = content
                .lines()
                .map(|line| {
                    if line.starts_with("Name=") {
                        format!("{line} Nightly")
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(&desktop.path, patched)?;
        }
        // Replace 'latest' with 'nightly' in update info
        config
            .update_info
            .as_ref()
            .map(|info| info.replace("|latest|", "|nightly|"))
    } else {
        config.update_info.clone()
    };

    // Add X-AppImage-* metadata to desktop entry. Uses the runtime arch so
    // the metadata reflects the actual binary, not a display alias.
    desktop.add_appimage_metadata(&app_name, version, &config.appimage_arch)?;

    // Compute output filename. Uses the display arch so projects can publish
    // under aliases like `amd64` while still pulling an `x86_64` runtime.
    let output_name = config
        .output_name
        .clone()
        .unwrap_or_else(|| desktop::compute_output_name(&app_name, Some(version), &config.arch));

    std::fs::create_dir_all(&config.output_dir)?;
    let output_path = config.output_dir.join(&output_name);

    // Resolve runtime
    let runtime_path = uruntime::resolve_runtime(config)?;
    crate::log_info!("Using runtime: {}", runtime_path.display());

    // Configure runtime (ELF section editing)
    uruntime::configure_runtime(
        &runtime_path,
        update_info.as_deref(),
        &config.env_vars,
        config.keep_mount,
    )?;

    // Resolve mkdwarfs
    let mkdwarfs = dwarfs::resolve_mkdwarfs(config)?;
    crate::log_info!("Using mkdwarfs: {}", mkdwarfs.display());

    // DWARFS profile optimization (optional)
    let profile = if config.optimize_launch {
        if config.source_date_epoch.is_some() {
            crate::log_warn!(
                "SOURCE_DATE_EPOCH is set, but OPTIMIZE_LAUNCH records a hotness \
                 profile by running the AppImage; the recorded profile is not \
                 reproducible"
            );
        }

        // Profiling launches the AppImage, which mounts via FUSE. Fail fast in
        // environments (typically minimal containers) where FUSE isn't usable.
        dwarfs::check_fuse_available()?;

        let tmp_appimage = util::process_unique_path(&config.tmpdir, ".analyze");
        dwarfs::build_profile_image(&mkdwarfs, &config.appdir, &runtime_path, &tmp_appimage)?;

        // Make it executable
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_appimage, std::fs::Permissions::from_mode(0o755))?;

        let profile_path = config
            .dwarfs_profile
            .as_ref()
            .cloned()
            .unwrap_or_else(|| config.appdir.join(".dwarfsprofile"));

        dwarfs::run_profiling(
            &tmp_appimage,
            &profile_path,
            &config.tmpdir,
            config.profile_timeout,
        )?;

        Some(profile_path)
    } else {
        config.dwarfs_profile.clone()
    };

    // Build the final AppImage
    dwarfs::build_appimage(
        &mkdwarfs,
        &config.appdir,
        &runtime_path,
        &output_path,
        &config.dwarfs_comp,
        config.source_date_epoch,
        profile.as_deref(),
    )?;

    // Make output executable
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&output_path, std::fs::Permissions::from_mode(0o755))?;

    // Generate zsync file if update info is set
    if update_info.is_some() {
        generate_zsync(&output_path, &output_name, &config.output_dir)?;
    }

    // Write appinfo file (runtime arch — matches the desktop entry metadata).
    write_appinfo(
        &config.output_dir,
        &app_name,
        version,
        &config.appimage_arch,
    )?;

    crate::log_info!("All done! AppImage at: {}", output_path.display());

    Ok(())
}

/// Generate a .zsync file using the zsync-rs library.
fn generate_zsync(appimage: &Path, filename: &str, output_dir: &Path) -> Result<()> {
    crate::log_info!("Generating .zsync file...");

    let mut file = std::fs::File::open(appimage)?;
    let control = ControlFile::generate(&mut file, filename, filename, None)?;

    let zsync_path = output_dir.join(format!("{filename}.zsync"));
    let mut out = std::fs::File::create(&zsync_path)?;
    control.write(&mut out)?;

    crate::log_info!("Wrote {}", zsync_path.display());
    Ok(())
}

/// Write an appinfo metadata file next to the output AppImage.
fn write_appinfo(output_dir: &Path, name: &str, version: &str, arch: &str) -> Result<()> {
    let path = output_dir.join("appinfo");
    let mut f = std::fs::File::create(&path)?;
    writeln!(f, "X-AppImage-Name={name}")?;
    writeln!(f, "X-AppImage-Version={version}")?;
    writeln!(f, "X-AppImage-Arch={arch}")?;
    Ok(())
}

/// Sort the AppDir's `.env` file so all lines starting with `unset`
/// come after every other line. The dotenv library used by sharun
/// requires this ordering.
fn sort_env_file(appdir: &Path) -> Result<()> {
    let env_path = appdir.join(".env");
    if !env_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&env_path)?;
    let mut regular = Vec::new();
    let mut unsets = Vec::new();

    for line in content.lines() {
        if line.starts_with("unset") {
            unsets.push(line);
        } else {
            regular.push(line);
        }
    }

    let mut sorted = String::new();
    for line in &regular {
        sorted.push_str(line);
        sorted.push('\n');
    }
    for line in &unsets {
        sorted.push_str(line);
        sorted.push('\n');
    }

    std::fs::write(&env_path, sorted)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_env_file_basic() {
        let tmp = std::env::temp_dir().join(format!(
            "appimagetool-test-env-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&tmp);

        let env_content = "unset BAZ\nexport FOO=bar\nunset QUX\nexport HELLO=world\n";
        std::fs::write(tmp.join(".env"), env_content).unwrap();

        sort_env_file(&tmp).unwrap();

        let sorted = std::fs::read_to_string(tmp.join(".env")).unwrap();
        let lines: Vec<&str> = sorted.lines().collect();

        // All non-unset lines first, then all unset lines
        let last_regular_idx = lines
            .iter()
            .enumerate()
            .rev()
            .find(|(_, l)| !l.starts_with("unset"))
            .map(|(i, _)| i);
        let first_unset_idx = lines
            .iter()
            .enumerate()
            .find(|(_, l)| l.starts_with("unset"))
            .map(|(i, _)| i);

        if let (Some(last_reg), Some(first_unset)) = (last_regular_idx, first_unset_idx) {
            assert!(
                last_reg < first_unset,
                "unset lines should come after regular lines"
            );
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_sort_env_file_no_env_file() {
        let tmp = std::env::temp_dir().join(format!(
            "appimagetool-test-noenv-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&tmp);

        // Should succeed even without .env
        assert!(sort_env_file(&tmp).is_ok());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_sort_env_file_all_unsets() {
        let tmp = std::env::temp_dir().join(format!(
            "appimagetool-test-allunset-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&tmp);

        std::fs::write(tmp.join(".env"), "unset A\nunset B\n").unwrap();
        sort_env_file(&tmp).unwrap();

        let sorted = std::fs::read_to_string(tmp.join(".env")).unwrap();
        assert!(sorted.lines().all(|l| l.starts_with("unset")));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_write_appinfo() {
        let tmp = std::env::temp_dir().join(format!(
            "appimagetool-test-appinfo-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&tmp);

        write_appinfo(&tmp, "TestApp", "1.0.0", "x86_64").unwrap();

        let content = std::fs::read_to_string(tmp.join("appinfo")).unwrap();
        assert!(content.contains("X-AppImage-Name=TestApp"));
        assert!(content.contains("X-AppImage-Version=1.0.0"));
        assert!(content.contains("X-AppImage-Arch=x86_64"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_write_appinfo_overwrite() {
        let tmp = std::env::temp_dir().join(format!(
            "appimagetool-test-appinfo2-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&tmp);

        write_appinfo(&tmp, "Old", "0.1", "x86_64").unwrap();
        write_appinfo(&tmp, "New", "2.0", "aarch64").unwrap();

        let content = std::fs::read_to_string(tmp.join("appinfo")).unwrap();
        assert!(content.contains("X-AppImage-Name=New"));
        assert!(content.contains("X-AppImage-Version=2.0"));
        assert!(content.contains("X-AppImage-Arch=aarch64"));
        assert!(!content.contains("Old"));
        assert!(!content.contains("0.1"));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
