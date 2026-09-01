use std::fs;
use std::path::PathBuf;

use appimagetool::config::{CliArgs, Config};
use appimagetool::desktop::{self, DesktopEntry};

/// Helper: create a temp dir that is automatically cleaned up.
struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Create a minimal valid AppDir in the given directory.
fn create_mock_appdir(dir: &std::path::Path, app_name: &str) {
    // AppRun
    fs::write(dir.join("AppRun"), "#!/bin/sh\nexec echo hello\n").unwrap();

    // .DirIcon (just a minimal PNG header)
    let png_header: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    fs::write(dir.join(".DirIcon"), png_header).unwrap();

    // .desktop file
    let desktop_content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={app_name}\n\
         Exec={app_name}\n\
         Icon={app_name}\n\
         Categories=Utility;\n"
    );
    fs::write(dir.join(format!("{app_name}.desktop")), desktop_content).unwrap();

    // A simple binary
    let bin_dir = dir.join("usr/bin");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::write(bin_dir.join(app_name), "#!/bin/sh\necho hello\n").unwrap();
}

// ─── Desktop entry tests ─────────────────────────────────────────────

#[test]
fn test_desktop_entry_parsing() {
    let tmp = TempDir::new("appimagetool-test");
    let appdir = tmp.path().join("AppDir");
    fs::create_dir_all(&appdir).unwrap();
    create_mock_appdir(&appdir, "TestApp");

    let desktop = DesktopEntry::from_appdir(&appdir).unwrap();
    assert_eq!(desktop.name, "TestApp");
    assert_eq!(desktop.exec, "TestApp");
    assert_eq!(desktop.icon_name.as_deref(), Some("TestApp"));
    assert_eq!(desktop.main_binary(), Some("TestApp"));
}

#[test]
fn test_desktop_entry_not_found() {
    let tmp = TempDir::new("appimagetool-test");
    let appdir = tmp.path().join("AppDir");
    fs::create_dir_all(&appdir).unwrap();
    // No .desktop file
    let result = DesktopEntry::from_appdir(&appdir);
    assert!(result.is_err());
}

#[test]
fn test_desktop_add_appimage_metadata() {
    let tmp = TempDir::new("appimagetool-test");
    let appdir = tmp.path().join("AppDir");
    fs::create_dir_all(&appdir).unwrap();
    create_mock_appdir(&appdir, "TestApp");

    let desktop = DesktopEntry::from_appdir(&appdir).unwrap();
    desktop
        .add_appimage_metadata("TestApp", "1.0.0", "x86_64")
        .unwrap();

    let content = fs::read_to_string(&desktop.path).unwrap();
    assert!(content.contains("X-AppImage-Name=TestApp"));
    assert!(content.contains("X-AppImage-Version=1.0.0"));
    assert!(content.contains("X-AppImage-Arch=x86_64"));
}

#[test]
fn test_desktop_metadata_idempotent() {
    let tmp = TempDir::new("appimagetool-test");
    let appdir = tmp.path().join("AppDir");
    fs::create_dir_all(&appdir).unwrap();
    create_mock_appdir(&appdir, "TestApp");

    let desktop = DesktopEntry::from_appdir(&appdir).unwrap();

    // Write metadata twice
    desktop
        .add_appimage_metadata("TestApp", "1.0.0", "x86_64")
        .unwrap();
    desktop
        .add_appimage_metadata("TestApp", "2.0.0", "x86_64")
        .unwrap();

    let content = fs::read_to_string(&desktop.path).unwrap();

    // Should NOT contain duplicate entries — old ones are removed
    assert_eq!(
        content.matches("X-AppImage-Version=").count(),
        1,
        "duplicate X-AppImage-Version entries found"
    );
    assert!(
        content.contains("X-AppImage-Version=2.0.0"),
        "should contain the latest version"
    );
}

#[test]
fn test_check_dir_icon_missing() {
    let tmp = TempDir::new("appimagetool-test");
    let appdir = tmp.path().join("AppDir");
    fs::create_dir_all(&appdir).unwrap();
    // No .DirIcon
    let result = DesktopEntry::check_dir_icon(&appdir);
    assert!(result.is_err());
}

#[test]
fn test_check_apprun_missing() {
    let tmp = TempDir::new("appimagetool-test");
    let appdir = tmp.path().join("AppDir");
    fs::create_dir_all(&appdir).unwrap();
    let result = DesktopEntry::check_apprun(&appdir);
    assert!(result.is_err());
}

// ─── Output name computation ─────────────────────────────────────────

#[test]
fn test_output_name_with_version() {
    let name = desktop::compute_output_name("MyApp", Some("2.0.1"), "x86_64");
    assert_eq!(name, "MyApp-2.0.1-anylinux-x86_64.AppImage");
}

#[test]
fn test_output_name_without_version() {
    let name = desktop::compute_output_name("MyApp", None, "aarch64");
    assert_eq!(name, "MyApp-anylinux-aarch64.AppImage");
}

#[test]
fn test_output_name_sanitizes_special_chars() {
    let name = desktop::compute_output_name("My App: Cool*", Some("1.0"), "x86_64");
    // "My App: Cool*" → "My_App__Cool_" → trimmed → "My_App__Cool"
    // Then "-1.0-anylinux-x86_64.AppImage" is appended by compute_output_name
    assert_eq!(name, "My_App__Cool-1.0-anylinux-x86_64.AppImage");
}

// ─── Config tests ────────────────────────────────────────────────────

#[test]
fn test_config_defaults() {
    let tmp = TempDir::new("appimagetool-test");
    let appdir = tmp.path().join("AppDir");
    fs::create_dir_all(&appdir).unwrap();

    let config = Config::from_cli_args(CliArgs {
        appdir: Some(appdir),
        tmpdir: Some(tmp.path().to_path_buf()),
        ..Default::default()
    })
    .unwrap();

    assert!(config.appdir.ends_with("AppDir"));
    assert_eq!(config.output_dir, PathBuf::from("."));
    assert!(!config.optimize_launch);
    assert!(!config.keep_mount);
    assert!(!config.devel_release);
    assert_eq!(config.profile_timeout, 10);
    // Default compression
    assert_eq!(config.dwarfs_comp, "zstd:level=22 -S26 -B6");
}

#[test]
fn test_config_display_arch_does_not_change_runtime_arch() {
    let tmp = TempDir::new("appimagetool-test");

    // Use an alias that can't accidentally match the host arch so we
    // can assert the runtime arch stayed at its host-default.
    let config = Config::from_cli_args(CliArgs {
        appdir: Some(tmp.path().join("AppDir")),
        arch: Some("amd64-alias".to_string()),
        tmpdir: Some(tmp.path().to_path_buf()),
        ..Default::default()
    })
    .unwrap();

    assert_eq!(config.arch, "amd64-alias");
    assert_ne!(config.appimage_arch, "amd64-alias");
}

#[test]
fn test_config_arch_falls_back_to_appimage_arch() {
    let tmp = TempDir::new("appimagetool-test-arch-fallback");

    let config = Config::from_cli_args(CliArgs {
        appdir: Some(tmp.path().join("AppDir")),
        appimage_arch: Some("aarch64".to_string()),
        tmpdir: Some(tmp.path().to_path_buf()),
        ..Default::default()
    })
    .unwrap();

    assert_eq!(config.appimage_arch, "aarch64");
    assert_eq!(config.arch, "aarch64");
}

#[test]
fn test_config_arch_alias_keeps_runtime_arch() {
    let tmp = TempDir::new("appimagetool-test-arch-alias");

    // Display arch like `amd64` must not affect the runtime arch we use
    // for downloads / X-AppImage-Arch metadata.
    let config = Config::from_cli_args(CliArgs {
        appdir: Some(tmp.path().join("AppDir")),
        appimage_arch: Some("x86_64".to_string()),
        arch: Some("amd64".to_string()),
        tmpdir: Some(tmp.path().to_path_buf()),
        ..Default::default()
    })
    .unwrap();

    assert_eq!(config.appimage_arch, "x86_64");
    assert_eq!(config.arch, "amd64");
}

// ─── Util tests ──────────────────────────────────────────────────────

#[test]
fn test_sanitize_filename() {
    use appimagetool::util;
    assert_eq!(util::sanitize_filename("hello world"), "hello_world");
    assert_eq!(util::sanitize_filename("app:name"), "app_name");
    assert_eq!(util::sanitize_filename("normal-name"), "normal-name");
    assert_eq!(util::sanitize_filename("trailing___"), "trailing");
    assert_eq!(util::sanitize_filename("a>b<c*d|e?f"), "a_b_c_d_e_f");
}

#[test]
fn test_is_elf() {
    use appimagetool::util;
    let tmp = TempDir::new("appimagetool-test");

    // Not ELF
    let not_elf = tmp.path().join("not_elf");
    fs::write(&not_elf, b"hello world").unwrap();
    assert!(!util::is_elf(&not_elf));

    // ELF magic
    let elf_file = tmp.path().join("elf");
    let mut data = vec![0u8; 64];
    data[0..4].copy_from_slice(b"\x7fELF");
    fs::write(&elf_file, data).unwrap();
    assert!(util::is_elf(&elf_file));

    // Non-existent file
    assert!(!util::is_elf(&tmp.path().join("no_such_file")));
}

// ─── Error display tests ─────────────────────────────────────────────

#[test]
fn test_error_messages() {
    use appimagetool::error::Error;
    let err = Error::NoDesktopEntry;
    assert!(err.to_string().contains(".desktop"));

    let err = Error::NoDirIcon;
    assert!(err.to_string().contains(".DirIcon"));

    let err = Error::NoAppRun;
    assert!(err.to_string().contains("AppRun"));

    let err = Error::SectionNotFound(".test".to_string());
    assert!(err.to_string().contains(".test"));
}

// ─── Full pipeline (requires mkdwarfs + uruntime, run with --ignored) ─

#[test]
#[ignore]
fn test_full_build_pipeline() {
    let tmp = TempDir::new("appimagetool-e2e");
    let appdir = tmp.path().join("AppDir");
    let output_dir = tmp.path().join("output");
    fs::create_dir_all(&appdir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    create_mock_appdir(&appdir, "HelloWorld");

    let config = Config::from_cli_args(CliArgs {
        appdir: Some(appdir.clone()),
        output: Some(output_dir.clone()),
        dwarfs_comp: Some("zstd:level=1".to_string()), // fast compression for tests
        tmpdir: Some(tmp.path().to_path_buf()),
        ..Default::default()
    })
    .unwrap();

    appimagetool::appimage::build(&config).unwrap();

    // Verify output exists and is an ELF file
    let mut found = false;
    for entry in fs::read_dir(&output_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".AppImage") {
            found = true;
            assert!(
                appimagetool::util::is_elf(&entry.path()),
                "output should be a valid ELF file"
            );
        }
    }
    assert!(found, "no .AppImage file found in output directory");
}
