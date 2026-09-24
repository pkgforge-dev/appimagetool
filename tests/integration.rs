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
fn test_config_normalizes_powerpc_arches() {
    // `env::consts::ARCH` is "powerpc64" for both powerpc targets, so the
    // explicit spellings must still normalize to the uname -m equivalents.
    for (input, expected) in [("powerpc64", "ppc64"), ("powerpc64le", "ppc64le")] {
        let tmp = TempDir::new("appimagetool-test-ppc-arch");
        let config = Config::from_cli_args(CliArgs {
            appdir: Some(tmp.path().join("AppDir")),
            appimage_arch: Some(input.to_string()),
            tmpdir: Some(tmp.path().to_path_buf()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(config.appimage_arch, expected, "input: {input}");
    }
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

// ─── uruntime download + checksum tests ─────────────────────────────

/// For every runtime arch the tool supports, download the pinned upstream
/// uruntime via `resolve_runtime` and verify the cached binary matches the
/// pinned SHA-256. Exercises the download URL, artifact name, and checksum
/// pinning end-to-end against the network.
#[test]
fn test_uruntime_download_checksum_each_arch() {
    use sha2::{Digest, Sha256};

    for (arch, expected) in appimagetool::pinned::URUNTIME_CHECKSUMS {
        let tmp = TempDir::new("appimagetool-uruntime-dl");
        let config = Config::from_cli_args(CliArgs {
            appdir: Some(tmp.path().join("AppDir")),
            appimage_arch: Some(arch.to_string()),
            tmpdir: Some(tmp.path().to_path_buf()),
            ..Default::default()
        })
        .unwrap();

        let runtime = appimagetool::uruntime::resolve_runtime(&config).unwrap_or_else(|e| {
            panic!("resolve_runtime failed for {arch}: {e}");
        });

        // The work copy must exist, be executable, and be a valid ELF.
        let work = fs::read(&runtime).unwrap_or_else(|e| {
            panic!("failed to read runtime for {arch}: {e}");
        });
        assert_eq!(&work[..4], b"\x7fELF", "{arch}: not an ELF binary");

        // Whichever route produced it, the resolved runtime must be the pinned
        // artifact byte for byte.
        let actual = format!("{:x}", Sha256::digest(&work));
        assert_eq!(actual, *expected, "{arch}: uruntime checksum mismatch");

        // An embedded blob short-circuits the cache, so only assert the cache
        // was populated when this build actually downloaded.
        if appimagetool::embed::uruntime(arch).is_none() {
            let cached = tmp.path().join(format!("uruntime-{arch}"));
            assert!(cached.exists(), "{arch}: cached binary missing");
            let file = fs::read(&cached).unwrap();
            assert_eq!(
                Sha256::digest(&work),
                Sha256::digest(&file),
                "{arch}: work copy differs from verified cache"
            );
        }
    }
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

// ─── Smoke test: package a trivial AppDir and run the AppImage ───────

/// A minimal AppDir whose `AppRun` prints `success`.
fn create_smoke_appdir(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(dir.join("usr/bin")).unwrap();
    let apprun = dir.join("AppRun");
    fs::write(&apprun, "#!/bin/sh\necho success\n").unwrap();
    fs::set_permissions(&apprun, fs::Permissions::from_mode(0o755)).unwrap();

    let png_header: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    fs::write(dir.join(".DirIcon"), png_header).unwrap();

    fs::write(
        dir.join("smoke.desktop"),
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Smoke\n\
         Exec=smoke\n\
         Icon=smoke\n\
         Categories=Utility;\n",
    )
    .unwrap();
}

/// The asset-architecture spelling of the machine running the tests, so it can
/// be compared against a requested runtime arch.
fn host_asset_arch() -> String {
    appimagetool::pinned::target_asset_arch(std::env::consts::ARCH, cfg!(target_endian = "little"))
}

/// The binfmt_misc handler name QEMU registers for `arch`, if it is one the
/// suite knows how to emulate.
fn binfmt_handler(arch: &str) -> Option<String> {
    match arch {
        "aarch64" | "riscv64" | "loongarch64" | "ppc64" | "ppc64le" => Some(format!("qemu-{arch}")),
        _ => None,
    }
}

/// Whether the kernel can execute `arch` binaries through a registered QEMU
/// handler. Native execution needs no handler.
fn binfmt_enabled(arch: &str) -> bool {
    let Some(handler) = binfmt_handler(arch) else {
        return false;
    };
    fs::read_to_string(format!("/proc/sys/fs/binfmt_misc/{handler}"))
        .is_ok_and(|entry| entry.contains("enabled"))
}

/// `ENOEXEC` — what exec returns when no binfmt handler matched the file.
const ENOEXEC: i32 = 8;

/// The ELF `EI_DATA` byte an architecture's binaries must carry.
///
/// `ppc64` is big-endian; `ppc64le` and every other architecture we ship are
/// little-endian.
fn expected_elf_data(arch: &str) -> Option<u8> {
    match arch {
        "ppc64" => Some(2),
        "x86_64" | "aarch64" | "riscv64" | "loongarch64" | "ppc64le" => Some(1),
        _ => None,
    }
}

/// The ELF `e_machine` value an architecture's binaries must carry.
///
/// `ppc64` and `ppc64le` deliberately share `EM_PPC64`; only [`expected_elf_data`]
/// separates them.
fn expected_emachine(arch: &str) -> Option<u16> {
    match arch {
        "x86_64" => Some(0x3e),
        "aarch64" => Some(0xb7),
        "riscv64" => Some(0xf3),
        "loongarch64" => Some(0x0102),
        "ppc64" | "ppc64le" => Some(0x15),
        _ => None,
    }
}

/// Read `e_machine` from an ELF header, honouring the header's own endianness.
fn elf_emachine(header: &[u8]) -> Option<u16> {
    let bytes: [u8; 2] = header.get(18..20)?.try_into().ok()?;
    match header.get(5)? {
        1 => Some(u16::from_le_bytes(bytes)),
        2 => Some(u16::from_be_bytes(bytes)),
        _ => None,
    }
}

/// The first 20 bytes of `path` — enough for the ELF identity fields.
fn elf_header(path: &std::path::Path) -> [u8; 20] {
    use std::io::Read;

    let mut header = [0u8; 20];
    fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .unwrap_or_else(|err| panic!("failed to read the ELF header of {}: {err}", path.display()));
    header
}

/// Package a minimal AppDir and execute the resulting AppImage, asserting that
/// its `AppRun` ran and that the embedded runtime matches the requested
/// architecture. This is the only test that exercises a finished AppImage the
/// way a user does, so it catches runtime/packaging regressions the other tests
/// cannot see.
///
/// Needs `mkdwarfs` for the host and network access to fetch the pinned
/// uruntime, so it is ignored by default; CI runs it per architecture with
/// `--ignored`. `APPIMAGETOOL_SMOKE_ARCH` selects the runtime architecture
/// (default: the host) and `APPIMAGETOOL_SMOKE_MKDWARFS` overrides the host
/// `mkdwarfs` path. Foreign architectures additionally need a binfmt_misc
/// handler that matches AppImages; see `.github/workflows/ci.yml` for why the
/// ones `qemu-user-static` ships do not.
#[test]
#[ignore = "packages and runs an AppImage; needs mkdwarfs, network, and QEMU for foreign arches"]
fn smoke_builds_and_runs_appimage() {
    let arch = std::env::var("APPIMAGETOOL_SMOKE_ARCH").unwrap_or_else(|_| host_asset_arch());
    let host = host_asset_arch();

    if arch != host && !binfmt_enabled(&arch) {
        panic!(
            "cannot run the {arch} AppImage on this {host} host: binfmt_misc has no enabled `{}` \
             handler. Install qemu-user-static for the interpreters, then register a handler whose \
             magic masks the EI_PAD bytes: an AppImage writes its \"AI\\x02\" magic there, so the \
             stock qemu-user-static entries never match it. See .github/workflows/ci.yml.",
            binfmt_handler(&arch).unwrap_or_else(|| format!("qemu-{arch}"))
        );
    }

    let tmp = TempDir::new("appimagetool-smoke");
    let appdir = tmp.path().join("AppDir");
    let output_dir = tmp.path().join("output");
    let build_tmp = tmp.path().join("build");
    let run_tmp = tmp.path().join("run");
    for dir in [&appdir, &output_dir, &build_tmp, &run_tmp] {
        fs::create_dir_all(dir).unwrap();
    }

    create_smoke_appdir(&appdir);

    let mut args = CliArgs {
        appdir: Some(appdir),
        output: Some(output_dir.clone()),
        appimage_arch: Some(arch.clone()),
        arch: Some(arch.clone()),
        dwarfs_comp: Some("zstd:level=1".to_string()), // fast compression for a smoke test
        tmpdir: Some(build_tmp),
        ..Default::default()
    };
    if let Some(mkdwarfs) = std::env::var_os("APPIMAGETOOL_SMOKE_MKDWARFS") {
        args.mkdwarfs = Some(PathBuf::from(mkdwarfs));
    }
    let config = Config::from_cli_args(args).unwrap();

    appimagetool::appimage::build(&config).unwrap();

    let appimage = fs::read_dir(&output_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("AppImage"))
        .expect("build produced no .AppImage");

    // The runtime we asked for has to be the one that landed in the AppImage.
    // An arch mix-up still runs natively on the host that built it and prints
    // `success`, so without this the test cannot tell x86_64 from aarch64 — the
    // shape of commit `1ed1dae`, where ppc64le was handed a big-endian ppc64
    // runtime. `ppc64` and `ppc64le` share `EM_PPC64`, so the endianness byte is
    // part of the assertion.
    let header = elf_header(&appimage);
    assert_eq!(&header[..4], b"\x7fELF", "{arch}: AppImage is not an ELF");
    assert_eq!(
        header[5],
        expected_elf_data(&arch).expect("known ELF endianness"),
        "{arch}: AppImage has the wrong EI_DATA byte"
    );
    assert_eq!(
        elf_emachine(&header),
        expected_emachine(&arch),
        "{arch}: AppImage carries the wrong e_machine"
    );

    // Mounting through FUSE is unavailable on CI runners and under QEMU's user
    // emulation, so drive the runtime's extraction path instead. The AppImage
    // is still executed end to end.
    let output = std::process::Command::new(&appimage)
        .env("TMPDIR", &run_tmp)
        .env("APPIMAGE_EXTRACT_AND_RUN", "1")
        .output()
        .unwrap_or_else(|err| {
            let hint = if arch != host && err.raw_os_error() == Some(ENOEXEC) {
                "\n  note: a binfmt_misc handler matched but produced no interpreter. The stock \
                 qemu-user-static magic requires the ELF padding bytes to be zero, and an AppImage \
                 writes \"AI\\x02\" there, so a handler that masks those bytes is needed."
            } else {
                ""
            };
            panic!("failed to execute {}: {err}{hint}", appimage.display());
        });

    assert!(
        output.status.success(),
        "{arch} AppImage exited with {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("success"),
        "{arch} AppImage did not print `success`\n--- stdout ---\n{}",
        String::from_utf8_lossy(&output.stdout),
    );
}
