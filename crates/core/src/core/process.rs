//! Detect running applications that hold caches open (Cursor, Xcode, Docker…).

use std::collections::HashSet;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System, UpdateKind};

/// Lower-cased names of all running processes.
pub fn running_process_names() -> HashSet<String> {
    let Ok(mut sys) = System::new_with_specifics(
        RefreshKind::nothing()
            .with_processes(ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet)),
    ) else {
        return HashSet::new();
    };
    let _ = sys.refresh_processes(ProcessesToUpdate::All, true);
    let mut names = HashSet::new();
    for p in sys.processes().values() {
        let name = p.name().to_string_lossy().to_ascii_lowercase();
        if !name.is_empty() {
            names.insert(name.clone());
        }
        // macOS app bundles: ".../Cursor.app/Contents/MacOS/Cursor" → "cursor.app"
        if let Some(exe) = p.exe() {
            for comp in exe.components() {
                let c = comp.as_os_str().to_string_lossy();
                if let Some(app) = c.strip_suffix(".app") {
                    names.insert(app.to_ascii_lowercase());
                }
            }
        }
    }
    names
}

/// Returns the display name of `app` if any of its process names is running.
pub fn running_app(app: &str, names: &HashSet<String>) -> Option<String> {
    if app.is_empty() {
        return None;
    }
    let candidates: Vec<String> = app
        .split('|')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    for c in &candidates {
        if names.contains(c)
            || names.contains(&format!("{c}.exe"))
            || names.contains(&format!("{c} helper"))
        {
            return Some(app.split('|').next().unwrap_or(app).trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_any_alias() {
        let names: HashSet<String> = ["cursor", "docker"].iter().map(|s| s.to_string()).collect();
        assert_eq!(running_app("Cursor", &names).as_deref(), Some("Cursor"));
        assert_eq!(running_app("Xcode|Simulator", &names), None);
        assert_eq!(
            running_app("Docker Desktop|docker", &names).as_deref(),
            Some("Docker Desktop")
        );
        assert_eq!(running_app("", &names), None);
    }

    #[test]
    fn lists_something() {
        assert!(!running_process_names().is_empty());
    }
}
