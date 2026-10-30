//! Report writers shared by the front ends: Markdown, JSON and CSV.

use std::path::Path;

use super::candidate::Candidate;
use super::fsutil::{compact_path, human_size};
use crate::scan::ScanResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Markdown,
    Json,
    Csv,
}

impl Format {
    pub fn parse(s: &str) -> Format {
        match s.to_ascii_lowercase().as_str() {
            "json" => Format::Json,
            "csv" => Format::Csv,
            _ => Format::Markdown,
        }
    }

    pub fn from_path(p: &Path) -> Format {
        match p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref()
        {
            Some("json") => Format::Json,
            Some("csv") => Format::Csv,
            _ => Format::Markdown,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Markdown => "md",
            Format::Json => "json",
            Format::Csv => "csv",
        }
    }
}

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Render a report of `items` (a subset of `result`) in the given format.
pub fn render(result: &ScanResult, items: &[&Candidate], home: &Path, format: Format) -> String {
    let total: u64 = items.iter().map(|c| c.size).sum();
    match format {
        Format::Json => serde_json::to_string_pretty(&serde_json::json!({
            "scanned_at": result.started_at,
            "projects": result.projects,
            "repos": result.repos,
            "guarded": result.guarded,
            "disk": result.disk,
            "volumes": result.volumes,
            "total_bytes": total,
            "candidates": items,
        }))
        .unwrap_or_default(),
        Format::Csv => {
            let mut s = String::from("id,label,category,risk,owner,size_bytes,size,size_exact,modified,volume,path_or_command\n");
            for c in items {
                s.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{},{},{}\n",
                    c.id,
                    csv_cell(&c.label),
                    c.category.as_str(),
                    c.risk.as_str(),
                    csv_cell(&c.owner_or_label()),
                    c.size,
                    human_size(c.size),
                    c.size_exact,
                    c.modified.map(|m| m.to_rfc3339()).unwrap_or_default(),
                    csv_cell(c.volume.as_deref().unwrap_or("")),
                    csv_cell(&c.detail())
                ));
            }
            s
        }
        Format::Markdown => {
            let mut s = String::new();
            s.push_str("# CleanProjects report\n\n");
            s.push_str(&format!(
                "Scanned {} · {} projects · {} repositories · {} guarded · disk {} free of {}\n\n",
                result.started_at.format("%Y-%m-%d %H:%M"),
                result.projects,
                result.repos,
                result.guarded,
                human_size(result.disk.free),
                human_size(result.disk.total)
            ));
            s.push_str("| Size | Risk | Category | Owner | Item | Path / command |\n|---:|---|---|---|---|---|\n");
            for c in items {
                s.push_str(&format!(
                    "| {} | {} | {} | {} | {} | `{}` |\n",
                    human_size(c.size),
                    c.risk.as_str(),
                    c.category.as_str(),
                    c.owner_or_label(),
                    c.label.replace('|', "\\|"),
                    compact_path(Path::new(&c.detail()), home)
                ));
            }
            s.push_str(&format!("\n**Total recoverable: {}**\n", human_size(total)));
            s
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::candidate::{Action, Category, Risk};
    use std::path::PathBuf;

    #[test]
    fn renders_all_formats() {
        let c = Candidate::new(
            "a, b",
            "d",
            Category::Temp,
            Risk::Low,
            Action::RemovePath {
                path: PathBuf::from("/h/x"),
            },
        )
        .with_size(2048, true);
        let result = ScanResult {
            candidates: vec![c.clone()],
            disk: Default::default(),
            volumes: vec![],
            roots: vec![],
            protected: vec![],
            projects: 0,
            repos: 0,
            guarded: 0,
            started_at: chrono::Local::now(),
            duration_ms: 0,
        };
        let items = vec![&c];
        let md = render(&result, &items, Path::new("/h"), Format::Markdown);
        assert!(md.contains("| 2.00 KB |") && md.contains("~/x"));
        let csv = render(&result, &items, Path::new("/h"), Format::Csv);
        assert!(csv.lines().nth(1).unwrap().contains("\"a, b\""));
        let json = render(&result, &items, Path::new("/h"), Format::Json);
        assert!(json.contains("\"total_bytes\": 2048"));
        assert_eq!(Format::from_path(Path::new("r.csv")), Format::Csv);
    }
}
