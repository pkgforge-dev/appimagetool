//! Parse and edit `.desktop` entry files inside an AppDir.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::util;

/// Parsed metadata from a .desktop file.
pub struct DesktopEntry {
    /// Path on disk to the parsed `.desktop` file.
    pub path: PathBuf,
    /// `Name=` value (preserves whitespace).
    pub name: String,
    /// `Exec=` value (preserves whitespace).
    pub exec: String,
    /// `Icon=` value, if present.
    pub icon_name: Option<String>,
}

impl DesktopEntry {
    /// Find and parse the single top-level .desktop file in an AppDir.
    pub fn from_appdir(appdir: &Path) -> Result<Self> {
        let mut found: Option<PathBuf> = None;
        for entry in
            std::fs::read_dir(appdir).map_err(|_| Error::AppDirNotFound(appdir.to_path_buf()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "desktop") && path.parent() == Some(appdir)
            {
                if found.is_some() {
                    return Err(Error::Config(
                        "multiple .desktop files found in AppDir, expected exactly one".to_string(),
                    ));
                }
                found = Some(path);
            }
        }

        let path = found.ok_or(Error::NoDesktopEntry)?;
        let content = std::fs::read_to_string(&path)?;

        let name = parse_key(&content, "Name").unwrap_or_default();
        let exec = parse_key(&content, "Exec").unwrap_or_default();
        let icon_name = parse_key(&content, "Icon");

        Ok(DesktopEntry {
            path,
            name,
            exec,
            icon_name,
        })
    }

    /// Append X-AppImage-* entries at the end of the [Desktop Entry] group.
    pub fn add_appimage_metadata(&self, app_name: &str, version: &str, arch: &str) -> Result<()> {
        let content = std::fs::read_to_string(&self.path)?;

        let mut out = String::new();
        let mut in_desktop_entry = false;
        let mut desktop_entry_end = None;
        // Remove old metadata, including entries misplaced in desktop actions.
        for line in content.lines().filter(|line| {
            !line.starts_with("X-AppImage-Name=")
                && !line.starts_with("X-AppImage-Version=")
                && !line.starts_with("X-AppImage-Arch=")
        }) {
            if line.starts_with('[') && line.ends_with(']') {
                if in_desktop_entry {
                    desktop_entry_end = Some(out.len());
                }
                in_desktop_entry = line == "[Desktop Entry]";
            }
            out.push_str(line);
            out.push('\n');
        }

        if in_desktop_entry {
            desktop_entry_end = Some(out.len());
        }
        let group_end = desktop_entry_end.ok_or_else(|| {
            Error::Config("desktop file is missing a [Desktop Entry] group".to_string())
        })?;

        // Keep a blank line before the metadata and before any following group.
        let entry_end = out[..group_end].trim_end_matches('\n').len();
        let mut metadata = format!(
            "\n\nX-AppImage-Name={app_name}\n\
             X-AppImage-Version={version}\n\
             X-AppImage-Arch={arch}\n"
        );
        if group_end < out.len() {
            metadata.push('\n');
        }
        out.replace_range(entry_end..group_end, &metadata);

        std::fs::write(&self.path, out)?;
        Ok(())
    }

    /// Get the main binary name from the Exec key.
    pub fn main_binary(&self) -> Option<&str> {
        let exec = self.exec.split_whitespace().next()?;
        let name = exec.trim_matches('"');
        let name = name.rsplit('/').next().unwrap_or(name);
        Some(name)
    }

    /// Verify .DirIcon exists in the AppDir.
    pub fn check_dir_icon(appdir: &Path) -> Result<()> {
        let dir_icon = appdir.join(".DirIcon");
        if dir_icon.exists() {
            Ok(())
        } else {
            Err(Error::NoDirIcon)
        }
    }

    /// Verify AppRun exists in the AppDir.
    pub fn check_apprun(appdir: &Path) -> Result<()> {
        let apprun = appdir.join("AppRun");
        if apprun.exists() {
            Ok(())
        } else {
            Err(Error::NoAppRun)
        }
    }
}

/// Compute the output filename.
pub fn compute_output_name(app_name: &str, version: Option<&str>, arch: &str) -> String {
    let app_name = util::sanitize_filename(app_name);
    match version {
        Some(v) if !v.is_empty() => {
            let v = util::sanitize_filename(v);
            format!("{app_name}-{v}-anylinux-{arch}.AppImage")
        }
        _ => {
            crate::log_warn!("VERSION is not set, omitting from filename");
            format!("{app_name}-anylinux-{arch}.AppImage")
        }
    }
}

/// Parse a key from a .desktop file. Preserves whitespace inside the value —
/// the desktop spec allows `Name=My App` and similar — but trims surrounding
/// quotes and trailing CR (for CRLF files).
fn parse_key(content: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix(&prefix) {
            let value = rest.trim_end_matches('\r').trim_matches('"');
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_basic() {
        let content = "[Desktop Entry]\nName=MyApp\nExec=myapp\nIcon=myapp\n";
        assert_eq!(parse_key(content, "Name"), Some("MyApp".to_string()));
        assert_eq!(parse_key(content, "Exec"), Some("myapp".to_string()));
        assert_eq!(parse_key(content, "Icon"), Some("myapp".to_string()));
        assert_eq!(parse_key(content, "Comment"), None);
    }

    #[test]
    fn test_parse_key_with_spaces() {
        let content = "Name=My App\nExec=/usr/bin/my app\n";
        assert_eq!(parse_key(content, "Name"), Some("My App".to_string()));
        assert_eq!(
            parse_key(content, "Exec"),
            Some("/usr/bin/my app".to_string())
        );
    }

    #[test]
    fn test_parse_key_strips_crlf() {
        let content = "Name=MyApp\r\nExec=myapp\r\n";
        assert_eq!(parse_key(content, "Name"), Some("MyApp".to_string()));
        assert_eq!(parse_key(content, "Exec"), Some("myapp".to_string()));
    }

    #[test]
    fn test_parse_key_quoted() {
        let content = "Exec=\"myapp\"\n";
        assert_eq!(parse_key(content, "Exec"), Some("myapp".to_string()));
    }

    #[test]
    fn test_parse_key_empty_value() {
        let content = "Name=\n";
        assert_eq!(parse_key(content, "Name"), Some("".to_string()));
    }

    #[test]
    fn test_parse_key_no_match() {
        let content = "Name=Test\n";
        assert_eq!(parse_key(content, "Version"), None);
    }

    #[test]
    fn test_main_binary_simple() {
        let de = DesktopEntry {
            path: PathBuf::from("/tmp/test.desktop"),
            name: "Test".to_string(),
            exec: "myapp".to_string(),
            icon_name: None,
        };
        assert_eq!(de.main_binary(), Some("myapp"));
    }

    #[test]
    fn test_main_binary_with_args() {
        let de = DesktopEntry {
            path: PathBuf::from("/tmp/test.desktop"),
            name: "Test".to_string(),
            exec: "myapp --flag value".to_string(),
            icon_name: None,
        };
        assert_eq!(de.main_binary(), Some("myapp"));
    }

    #[test]
    fn test_main_binary_with_path() {
        let de = DesktopEntry {
            path: PathBuf::from("/tmp/test.desktop"),
            name: "Test".to_string(),
            exec: "/usr/bin/myapp".to_string(),
            icon_name: None,
        };
        assert_eq!(de.main_binary(), Some("myapp"));
    }

    #[test]
    fn test_main_binary_quoted_path() {
        let de = DesktopEntry {
            path: PathBuf::from("/tmp/test.desktop"),
            name: "Test".to_string(),
            exec: "\"/opt/myapp\"".to_string(),
            icon_name: None,
        };
        assert_eq!(de.main_binary(), Some("myapp"));
    }

    #[test]
    fn test_compute_output_name_basic() {
        let name = compute_output_name("MyApp", Some("1.0"), "x86_64");
        assert_eq!(name, "MyApp-1.0-anylinux-x86_64.AppImage");
    }

    #[test]
    fn test_compute_output_name_empty_version() {
        let name = compute_output_name("MyApp", Some(""), "x86_64");
        // Empty version is treated like None
        assert_eq!(name, "MyApp-anylinux-x86_64.AppImage");
    }

    #[test]
    fn test_compute_output_name_no_version() {
        let name = compute_output_name("MyApp", None, "aarch64");
        assert_eq!(name, "MyApp-anylinux-aarch64.AppImage");
    }

    #[test]
    fn test_compute_output_name_sanitizes_version() {
        let name = compute_output_name("MyApp", Some("1.0:beta"), "x86_64");
        assert_eq!(name, "MyApp-1.0_beta-anylinux-x86_64.AppImage");
    }
}
