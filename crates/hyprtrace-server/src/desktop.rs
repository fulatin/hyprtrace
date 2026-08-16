//! Resolve bare window classes (e.g. "code", "firefox") to human-friendly
//! display names and icon hints using the freedesktop.org `.desktop` files
//! installed on the system.
//!
//! Parsing is intentionally hand-rolled (no extra dependencies) and only
//! inspects the keys HyprTrace cares about: `Name`, `Icon`, `StartupWMClass`,
//! `NoDisplay` and `Hidden`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Metadata resolved for a single application from a `.desktop` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppMetadata {
    pub desktop_id: String,
    pub display_name: String,
    pub icon: String,
}

/// Parsed view of one `.desktop` file, including the extra lookup keys that
/// are not part of the public [`AppMetadata`].
#[derive(Debug, Clone)]
struct DesktopEntry {
    metadata: AppMetadata,
    startup_wm_class: Option<String>,
    stem: String,
    name: String,
}

/// Scans the standard XDG application directories and builds a lookup table
/// of lowercased lookup keys -> [`AppMetadata`].
#[derive(Debug, Default)]
pub struct AppMetadataResolver {
    /// Lookup keys (lowercased) -> metadata.
    entries: HashMap<String, AppMetadata>,
}

impl AppMetadataResolver {
    /// Build a resolver by scanning the standard application directories.
    /// Missing directories are silently ignored.
    pub fn scan() -> Self {
        let mut resolver = Self::default();
        for dir in scan_directories() {
            resolver.scan_dir(&dir);
        }
        resolver
    }

    fn scan_dir(&mut self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return, // Missing / unreadable directory: skip.
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            if let Some(entry) = parse_desktop_file(&path) {
                self.insert(entry);
            }
        }
    }

    /// Register an entry under all of its lookup keys.
    ///
    /// More specific keys are inserted first, and later files never override
    /// a key that is already present — so a `StartupWMClass` match always
    /// wins over a stem or name match, and earlier directories (system-wide,
    /// more authoritative ordering) win over later ones.
    fn insert(&mut self, entry: DesktopEntry) {
        let mut keys: Vec<String> = Vec::new();
        if let Some(wmclass) = entry.startup_wm_class {
            keys.push(wmclass.to_lowercase());
        }
        let stem = entry.stem.to_lowercase();
        if !stem.is_empty() {
            keys.push(stem);
        }
        keys.push(entry.name.to_lowercase());

        for key in keys {
            let key = key.trim().to_string();
            if key.is_empty() {
                continue;
            }
            self.entries.entry(key).or_insert_with(|| entry.metadata.clone());
        }
    }

    /// Look up a window class, normalized to lowercase. Returns the first
    /// matching entry (StartupWMClass, then desktop file stem, then Name).
    pub fn lookup(&self, class: &str) -> Option<AppMetadata> {
        let key = class.trim().to_lowercase();
        if key.is_empty() {
            return None;
        }
        self.entries.get(&key).cloned()
    }
}

/// Process-wide, lazily-initialized resolver so the `.desktop` directories are
/// scanned once instead of on every request. The first `global()` call performs
/// the filesystem I/O; subsequent calls return the cached instance.
pub fn global() -> &'static AppMetadataResolver {
    static RESOLVER: OnceLock<AppMetadataResolver> = OnceLock::new();
    RESOLVER.get_or_init(AppMetadataResolver::scan)
}

/// Directories to scan, in order. Missing directories are ignored by the
/// caller, so constructing the list always succeeds.
fn scan_directories() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }
    dirs
}

/// Parse one `.desktop` file. Returns `None` when the file is missing,
/// unreadable, has no plain `Name`, or is marked `NoDisplay=true` /
/// `Hidden=true`.
///
/// Only the values inside the `[Desktop Entry]` group are considered, and
/// only the plain `Name` key is used (localized `Name[xx]` keys are ignored).
fn parse_desktop_file(path: &Path) -> Option<DesktopEntry> {
    let content = std::fs::read_to_string(path).ok()?;

    let mut in_desktop_entry = false;
    let mut name: Option<String> = None;
    let mut icon: Option<String> = None;
    let mut startup_wm_class: Option<String> = None;
    let mut no_display = false;
    let mut hidden = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip blank lines and comments.
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }

        // Group header.
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let group = &trimmed[1..trimmed.len() - 1];
            in_desktop_entry = group == "Desktop Entry";
            continue;
        }

        if !in_desktop_entry {
            continue;
        }

        let (key, value) = match trimmed.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => continue,
        };

        match key {
            // Only the plain `Name`; localized `Name[xx]` never overrides it.
            "Name" => {
                if name.is_none() {
                    name = Some(value.to_string());
                }
            }
            "Icon" => {
                if icon.is_none() {
                    icon = Some(value.to_string());
                }
            }
            "StartupWMClass" => {
                if startup_wm_class.is_none() {
                    startup_wm_class = Some(value.to_string());
                }
            }
            "NoDisplay" => {
                if value.eq_ignore_ascii_case("true") {
                    no_display = true;
                }
            }
            "Hidden" => {
                if value.eq_ignore_ascii_case("true") {
                    hidden = true;
                }
            }
            _ => {}
        }
    }

    if no_display || hidden {
        return None;
    }

    let name = name?;
    let icon = icon.unwrap_or_default();

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    // The convention is that the `.desktop` file name is the desktop id.
    let desktop_id = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&stem)
        .to_string();

    Some(DesktopEntry {
        metadata: AppMetadata {
            desktop_id,
            display_name: name.clone(),
            icon,
        },
        startup_wm_class,
        stem,
        name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(dir: &Path, file_name: &str, content: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(file_name), content).unwrap();
    }

    /// Build a resolver over only the given temp directory (used to isolate
    /// tests from the host system's real applications).
    fn resolver_over(dir: &Path) -> AppMetadataResolver {
        let mut resolver = AppMetadataResolver::default();
        resolver.scan_dir(dir);
        resolver
    }

    /// A unique, per-test temp directory so parallel tests don't clobber each
    /// other. Uses the global test counter-like approach via process id + a
    /// unique suffix supplied by the caller.
    fn unique_dir(suffix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hyprtrace-desktop-test-{}-{}",
            std::process::id(),
            suffix
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn no_display_entries_are_skipped() {
        let dir = unique_dir("nodisplay");
        write_file(
            &dir,
            "hidden-app.desktop",
            "[Desktop Entry]\nName=Hidden App\nIcon=hidden\nNoDisplay=true\n",
        );
        write_file(
            &dir,
            "hidden-flag.desktop",
            "[Desktop Entry]\nName=Hidden Flag\nIcon=flag\nHidden=true\n",
        );

        let resolver = resolver_over(&dir);
        assert!(resolver.lookup("hidden app").is_none());
        assert!(resolver.lookup("hidden-app").is_none());
        assert!(resolver.lookup("hidden flag").is_none());
        assert!(resolver.lookup("hidden-flag").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn startup_wm_class_matches_and_wins_over_other_keys() {
        let dir = unique_dir("wmclass");
        write_file(
            &dir,
            "code.desktop",
            "[Desktop Entry]\nName=Visual Studio Code\nStartupWMClass=Code\nIcon=vscode\n",
        );

        let resolver = resolver_over(&dir);
        let meta = resolver.lookup("CODE").expect("startup WM class should match");
        assert_eq!(meta.desktop_id, "code.desktop");
        assert_eq!(meta.display_name, "Visual Studio Code");
        assert_eq!(meta.icon, "vscode");

        let meta = resolver.lookup("Code").expect("case should be normalized");
        assert_eq!(meta.display_name, "Visual Studio Code");

        // Desktop file stem also resolves.
        let meta = resolver.lookup("code").expect("file stem should match");
        assert_eq!(meta.display_name, "Visual Studio Code");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stem_match_and_localized_name_ignored() {
        let dir = unique_dir("stem");
        write_file(
            &dir,
            "firefox.desktop",
            "[Desktop Entry]\nName=Firefox\nName[zh_CN]=火狐浏览器\nIcon=firefox\n",
        );

        let resolver = resolver_over(&dir);
        // Stem match.
        let meta = resolver.lookup("firefox").expect("stem should match");
        assert_eq!(meta.display_name, "Firefox", "localized Name must not override plain Name");
        assert_eq!(meta.icon, "firefox");

        // Name fallback (lowercased plain Name) also matches.
        let meta = resolver.lookup("FIREFOX").expect("name fallback should match");
        assert_eq!(meta.display_name, "Firefox");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_class_returns_none() {
        let dir = unique_dir("unknown");
        write_file(
            &dir,
            "code.desktop",
            "[Desktop Entry]\nName=Visual Studio Code\n",
        );

        let resolver = resolver_over(&dir);
        assert!(resolver.lookup("totally-unknown").is_none());
        assert!(resolver.lookup("").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
