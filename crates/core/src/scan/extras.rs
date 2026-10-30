//! Path-derived candidates that are not simple rules: installer files in
//! Downloads, superseded version folders, trash bins on external volumes.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;

use crate::core::candidate::{Action, Candidate, Category, Risk};
use crate::core::disk::{Volume, VolumeKind};
use crate::core::fsutil::{dir_size, measure, modified_at};

const INSTALLER_EXT: &[&str] = &[
    "dmg", "pkg", "iso", "msi", "exe", "appimage", "deb", "rpm", "zip", "xip",
];

/// Installer and archive files in Downloads older than `min_days`.
pub fn installers(home: &Path, min_days: u64) -> Vec<Candidate> {
    let mut out = Vec::new();
    for dir in [home.join("Downloads"), home.join("İndirilenler")] {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            let Ok(meta) = e.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let ext = path
                .extension()
                .map(|x| x.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if !INSTALLER_EXT.contains(&ext.as_str()) {
                continue;
            }
            let modified = modified_at(&path);
            let age = modified
                .map(|m| Local::now().signed_duration_since(m).num_days().max(0) as u64)
                .unwrap_or(0);
            if age < min_days || meta.len() < 8 * 1024 * 1024 {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            out.push(
                Candidate::new(
                    format!("Installer {name}"),
                    format!(
                        "{} file in Downloads, {age} days old. Installers can be downloaded again.",
                        ext.to_uppercase()
                    ),
                    Category::Temp,
                    Risk::Low,
                    Action::RemovePath { path: path.clone() },
                )
                .with_size(meta.len(), true)
                .with_modified(modified)
                .with_owner("Downloads")
                .with_tag("installer"),
            );
        }
    }
    out
}

fn version_key(name: &str) -> Vec<u64> {
    name.split(|c: char| !c.is_ascii_digit())
        .filter_map(|p| p.parse().ok())
        .collect()
}

/// Keep the newest of several versioned folders, offer the rest.
fn superseded(
    dir: &Path,
    label: &str,
    owner: &str,
    category: Category,
    risk: Risk,
    description: &str,
) -> Vec<Candidate> {
    let Ok(rd) = fs::read_dir(dir) else {
        return vec![];
    };
    let mut groups: std::collections::BTreeMap<String, Vec<PathBuf>> = Default::default();
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        let family: String = name
            .trim_end_matches(|c: char| {
                c.is_ascii_digit()
                    || c == '.'
                    || c == ' '
                    || c == '-'
                    || c == '_'
                    || c == '('
                    || c == ')'
            })
            .to_string();
        groups.entry(family).or_default().push(p);
    }
    let mut out = Vec::new();
    for (_, mut paths) in groups {
        if paths.len() < 2 {
            continue;
        }
        paths.sort_by_key(|p| {
            version_key(
                &p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
            )
        });
        let newest = paths.pop().unwrap();
        for p in paths {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let newest_name = newest
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            out.push(
                Candidate::new(
                    format!("{label} {name}"),
                    format!("{description} Newer version present: {newest_name}."),
                    category,
                    risk,
                    Action::RemovePath { path: p.clone() },
                )
                .with_measure(measure(&p))
                .with_modified(modified_at(&p))
                .with_owner(owner)
                .with_tag("superseded"),
            );
        }
    }
    out
}

/// Old iOS DeviceSupport folders and JetBrains product versions.
pub fn superseded_versions(home: &Path) -> Vec<Candidate> {
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        out.extend(superseded(
            &home.join("Library/Developer/Xcode/iOS DeviceSupport"),
            "Old iOS DeviceSupport",
            "Xcode",
            Category::Mobile,
            Risk::Low,
            "Debug symbols for an iOS version you no longer connect. Re-downloaded if such a device connects again.",
        ));
        for sub in [
            "Library/Application Support/JetBrains",
            "Library/Caches/JetBrains",
            "Library/Logs/JetBrains",
        ] {
            out.extend(superseded(
                &home.join(sub),
                "Old JetBrains version",
                "JetBrains",
                Category::Ide,
                Risk::Medium,
                "Settings, caches or logs of a previous IDE version.",
            ));
        }
    } else if cfg!(windows) {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            out.extend(superseded(
                &Path::new(&local).join("JetBrains"),
                "Old JetBrains version",
                "JetBrains",
                Category::Ide,
                Risk::Medium,
                "Caches of a previous IDE version.",
            ));
        }
    } else {
        for sub in [
            ".config/JetBrains",
            ".cache/JetBrains",
            ".local/share/JetBrains",
        ] {
            out.extend(superseded(
                &home.join(sub),
                "Old JetBrains version",
                "JetBrains",
                Category::Ide,
                Risk::Medium,
                "Settings or caches of a previous IDE version.",
            ));
        }
    }
    out
}

/// Trash folders and leftover recovery bins on external volumes.
pub fn volume_extras(vols: &[Volume]) -> Vec<Candidate> {
    let mut out = Vec::new();
    for v in vols
        .iter()
        .filter(|v| v.kind == VolumeKind::External && !v.read_only)
    {
        let trashes = v.mount.join(".Trashes");
        if let Ok(rd) = fs::read_dir(&trashes) {
            for e in rd.flatten() {
                let p = e.path();
                if !p.is_dir() {
                    continue;
                }
                let size = dir_size(&p);
                if size == 0 {
                    continue;
                }
                out.push(
                    Candidate::new(
                        format!("Trash on {}", v.name),
                        "Files already moved to the trash on this external volume. Emptying is irreversible.",
                        Category::Temp,
                        Risk::Medium,
                        Action::RemovePath { path: p },
                    )
                    .with_size(size, true)
                    .with_owner("System")
                    .with_tag("volume"),
                );
            }
        }
        let recycle = v.mount.join("$RECYCLE.BIN");
        if recycle.is_dir() {
            let size = dir_size(&recycle);
            if size > 0 {
                out.push(
                    Candidate::new(
                        format!("Recycle bin on {}", v.name),
                        "Windows recycle bin on this volume.",
                        Category::Temp,
                        Risk::Medium,
                        Action::RemovePath { path: recycle },
                    )
                    .with_size(size, true)
                    .with_owner("System")
                    .with_tag("volume"),
                );
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installers_filter_by_age_and_type() {
        let tmp = tempfile::tempdir().unwrap();
        let dl = tmp.path().join("Downloads");
        fs::create_dir_all(&dl).unwrap();
        fs::write(dl.join("app.dmg"), vec![0u8; 9 * 1024 * 1024]).unwrap();
        fs::write(dl.join("notes.txt"), vec![0u8; 9 * 1024 * 1024]).unwrap();
        let fresh = installers(tmp.path(), 7);
        assert!(fresh.is_empty(), "a brand-new dmg is not offered");
        let old = installers(tmp.path(), 0);
        assert_eq!(old.len(), 1);
        assert!(old[0].label.contains("app.dmg"));
    }

    #[test]
    fn superseded_keeps_newest() {
        let tmp = tempfile::tempdir().unwrap();
        for v in [
            "IntelliJIdea2023.3",
            "IntelliJIdea2024.1",
            "IntelliJIdea2024.2",
            "WebStorm2024.2",
        ] {
            fs::create_dir_all(tmp.path().join(v)).unwrap();
            fs::write(tmp.path().join(v).join("f"), b"x").unwrap();
        }
        let out = superseded(
            tmp.path(),
            "Old",
            "JetBrains",
            Category::Ide,
            Risk::Medium,
            "d",
        );
        let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(out.len(), 2, "{labels:?}");
        assert!(labels.contains(&"Old IntelliJIdea2023.3"));
        assert!(labels.contains(&"Old IntelliJIdea2024.1"));
    }
}
