// Pinned upstream artifact URLs and SHA-256 digests.
//
// Uses line comments rather than `//!` module docs because `build.rs` pulls this
// file in with `include!`, which cannot carry inner doc comments. The module
// documentation lives on the `pub mod pinned;` declaration in `lib.rs`.

/// Default uruntime download URL. Pinned for reproducible builds; override with
/// `--runtime-url` / `URUNTIME_LINK`. `{arch}` is replaced with the target
/// architecture.
///
/// The `-lite` in this asset name is a licensing boundary, not just a size
/// choice. The lite variant embeds DwarFS `dwarfs-fuse-extract`, which holds
/// only the MIT-licensed reader and extractor. The non-lite variant embeds
/// `dwarfs-universal`, which also contains the GPL-3.0 `mkdwarfs` writer.
pub const URUNTIME_URL_TEMPLATE: &str = "https://github.com/VHSgunzo/uruntime/releases/download/v0.8.1/uruntime-appimage-dwarfs-lite-{arch}";

/// Default mkdwarfs download URL. `{arch}` is replaced with the target
/// architecture.
///
/// `dwarfs-universal` contains the GPL-3.0 `mkdwarfs` writer. Fetching it at
/// run time leaves upstream as its distributor, which is why this is the
/// default. Embedding it with `--features embed-mkdwarfs` makes the resulting
/// appimagetool binary a GPL-3.0 work; see `licenses/mkdwarfs-GPL-3.0.txt`.
pub const MKDWARFS_URL_TEMPLATE: &str =
    "https://github.com/mhx/dwarfs/releases/download/v0.15.6/dwarfs-universal-0.15.6-Linux-{arch}";

/// SHA-256 of each `uruntime-appimage-dwarfs-lite-{arch}` asset from the pinned
/// release. Only enforced for [`URUNTIME_URL_TEMPLATE`]; a user-supplied
/// `--runtime-url` points at an unknown artifact and skips verification.
pub const URUNTIME_CHECKSUMS: &[(&str, &str)] = &[
    (
        "aarch64",
        "c1641dfe465f4cb70ae545fbfb9de7a221aa6e9a0b1d7ea600b213a1f10738a0",
    ),
    (
        "loongarch64",
        "7f149441fbb772477c8748e58c1233e4e0e141d6ca2507c4ddad97b69b4008bc",
    ),
    (
        "ppc64",
        "db7a7834c1cb657c2708eab206d9a140231962d22f776cd0ea4fd9a482be3037",
    ),
    (
        "ppc64le",
        "0fa3983c5c18794e841d2ad20938b87c467cbf24034a0fad608c5fe00a1668a9",
    ),
    (
        "riscv64",
        "25d1137942a24fed3d1126686960de5dc5ec44070d5e3a6c3544cca229d230cb",
    ),
    (
        "x86_64",
        "b3c2916153e089d703cee5a7ebc540941d2f1d71baa90eb3092b15eef48345a8",
    ),
];

/// SHA-256 of each `dwarfs-universal-{version}-Linux-{arch}` asset from the
/// pinned release, taken from the upstream release notes.
pub const MKDWARFS_CHECKSUMS: &[(&str, &str)] = &[
    (
        "aarch64",
        "2d513c43ad652163e5e31c1a96f3c736e7e188e661ff62fc9143ea7147bd936b",
    ),
    (
        "loongarch64",
        "16d3d9df8cedcf7690006006f6d994545fa096204e7fc78ef567234026e4fec6",
    ),
    (
        "ppc64",
        "cfd7dd8dcada898d4f4f94bcf671596fd868dd043f00d47fd394e633444dcf37",
    ),
    (
        "ppc64le",
        "2b71960672c108e413ef473b8a726feb858782ff3c9c28b6f4616e10097b0452",
    ),
    (
        "riscv64",
        "b20be0530cbf1662d6b9e5ba27a57537e3bf50f90cdd6bb7922327b62f5f9417",
    ),
    (
        "x86_64",
        "50891c38ba359db8271819a6cbf6aaa8068681523f0c4f2b8242007a45edaa28",
    ),
];

/// DwarFS source distribution for the pinned `mkdwarfs` release.
///
/// A build with `embed-mkdwarfs` conveys GPL-3.0 code, and GPL-3.0 section 6
/// requires its Corresponding Source to stay available to recipients. The
/// release workflow mirrors this tarball alongside the binaries rather than
/// relying on the upstream URL, which upstream could retag or remove.
pub const MKDWARFS_SOURCE_URL: &str =
    "https://github.com/mhx/dwarfs/releases/download/v0.15.6/dwarfs-0.15.6.tar.xz";

/// SHA-256 of [`MKDWARFS_SOURCE_URL`].
pub const MKDWARFS_SOURCE_SHA256: &str =
    "087b77c1d6a1f253df896b054f95ef17469c63b00be51f4d081633cc8817481c";

/// Substitute `{arch}` in a pinned URL template.
pub fn url_for(template: &str, arch: &str) -> String {
    template.replace("{arch}", arch)
}

/// Look up a pinned SHA-256 digest by architecture.
pub fn checksum_for(table: &[(&'static str, &'static str)], arch: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|(table_arch, _)| *table_arch == arch)
        .map(|(_, digest)| *digest)
}

/// Normalize a Rust target architecture to the `uname -m` spelling upstream
/// uses in its release asset names.
pub fn normalize_asset_arch(arch: &str) -> String {
    match arch {
        "powerpc64" => "ppc64".to_string(),
        "powerpc64le" => "ppc64le".to_string(),
        other => other.to_string(),
    }
}

/// Resolve the upstream asset architecture for a Cargo build target.
///
/// Cargo reports `powerpc64` in `CARGO_CFG_TARGET_ARCH` for both endiannesses,
/// so the endianness has to be consulted separately to tell `ppc64` from
/// `ppc64le`. Getting this wrong hands a big-endian runtime to a little-endian
/// target, which is the failure commit `1ed1dae` fixed.
pub fn target_asset_arch(arch: &str, little_endian: bool) -> String {
    if arch == "powerpc64" && little_endian {
        "ppc64le".to_string()
    } else {
        normalize_asset_arch(arch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_maps_rust_spellings_to_asset_names() {
        assert_eq!(normalize_asset_arch("powerpc64"), "ppc64");
        assert_eq!(normalize_asset_arch("powerpc64le"), "ppc64le");
        assert_eq!(normalize_asset_arch("x86_64"), "x86_64");
        assert_eq!(normalize_asset_arch("aarch64"), "aarch64");
    }

    #[test]
    fn target_arch_resolves_powerpc_by_endianness() {
        assert_eq!(target_asset_arch("powerpc64", true), "ppc64le");
        assert_eq!(target_asset_arch("powerpc64", false), "ppc64");
        // Endianness must not perturb any other architecture.
        for arch in ["x86_64", "aarch64", "riscv64", "loongarch64"] {
            assert_eq!(target_asset_arch(arch, true), arch);
            assert_eq!(target_asset_arch(arch, false), arch);
        }
    }

    #[test]
    fn every_release_target_has_both_pins() {
        // The release matrix builds these six; each needs a uruntime and an
        // mkdwarfs digest or the embed features cannot serve that target.
        for arch in [
            "x86_64",
            "aarch64",
            "riscv64",
            "loongarch64",
            "ppc64",
            "ppc64le",
        ] {
            assert!(
                checksum_for(URUNTIME_CHECKSUMS, arch).is_some(),
                "{arch}: missing uruntime digest"
            );
            assert!(
                checksum_for(MKDWARFS_CHECKSUMS, arch).is_some(),
                "{arch}: missing mkdwarfs digest"
            );
        }
        assert!(checksum_for(URUNTIME_CHECKSUMS, "m68k").is_none());
    }

    #[test]
    fn pins_are_well_formed() {
        for (table, name) in [
            (URUNTIME_CHECKSUMS, "uruntime"),
            (MKDWARFS_CHECKSUMS, "mkdwarfs"),
        ] {
            for (arch, digest) in table {
                assert_eq!(digest.len(), 64, "{name}/{arch}: not a sha256");
                assert!(
                    digest
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
                    "{name}/{arch}: digest must be lowercase hex"
                );
            }
        }
    }

    #[test]
    fn url_templates_substitute_arch_over_https() {
        for template in [URUNTIME_URL_TEMPLATE, MKDWARFS_URL_TEMPLATE] {
            assert!(template.starts_with("https://"));
            let url = url_for(template, "ppc64le");
            assert!(url.contains("ppc64le"), "{url}");
            assert!(!url.contains("{arch}"), "{url}");
        }
    }

    #[test]
    fn corresponding_source_matches_the_pinned_mkdwarfs_release() {
        // Mirroring the source for a different release than the binary embeds
        // would leave a GPL-3.0 build without its Corresponding Source, so the
        // two pins must name the same version.
        let version = MKDWARFS_URL_TEMPLATE
            .split("/download/v")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .expect("mkdwarfs URL must carry a version");
        assert!(
            MKDWARFS_SOURCE_URL.contains(&format!("/download/v{version}/")),
            "source pin {MKDWARFS_SOURCE_URL} does not match mkdwarfs v{version}"
        );
        assert!(MKDWARFS_SOURCE_URL.ends_with(".tar.xz"));
        assert_eq!(MKDWARFS_SOURCE_SHA256.len(), 64);
    }

    #[test]
    fn uruntime_pin_stays_on_the_mit_lite_variant() {
        // The non-lite variant bundles dwarfs-universal, which carries the
        // GPL-3.0 mkdwarfs writer. Switching the pin would silently pull GPL
        // code into every `embed-uruntime` build.
        assert!(URUNTIME_URL_TEMPLATE.contains("dwarfs-lite"));
    }
}
