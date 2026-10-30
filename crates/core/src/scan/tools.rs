//! Candidates that require external tools (docker, podman, pnpm, simctl...).

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use crate::core::candidate::{Action, Candidate, Category, Risk};
use crate::core::fsutil::{dir_size, parse_human_size};

const TOOL_TIMEOUT: Duration = Duration::from_secs(15);

/// Run a command, killing it when it exceeds `timeout`.
pub fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Option<Output> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let mut stderr = child.stderr.take()?;
    let out_thread = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = stdout.read_to_end(&mut v);
        v
    });
    let err_thread = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = stderr.read_to_end(&mut v);
        v
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => return None,
        }
    };
    Some(Output {
        status,
        stdout: out_thread.join().unwrap_or_default(),
        stderr: err_thread.join().unwrap_or_default(),
    })
}

fn tool_output(program: &str, args: &[&str]) -> Option<String> {
    which::which(program).ok()?;
    let mut cmd = Command::new(program);
    cmd.args(args);
    let out = run_with_timeout(cmd, TOOL_TIMEOUT)?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Parse `docker system df --format "{{.Type}}\t{{.Reclaimable}}"`.
pub fn parse_system_df(text: &str) -> Vec<(String, u64)> {
    text.lines()
        .filter_map(|line| {
            let (kind, reclaim) = line.split_once('\t')?;
            let size = reclaim
                .split_whitespace()
                .next()
                .and_then(parse_human_size)?;
            Some((kind.trim().to_string(), size))
        })
        .collect()
}

fn container_engine(program: &str, label: &str) -> Vec<Candidate> {
    let Some(text) = tool_output(
        program,
        &["system", "df", "--format", "{{.Type}}\t{{.Reclaimable}}"],
    ) else {
        return Vec::new();
    };
    let rows = parse_system_df(&text);
    let get = |k: &str| {
        rows.iter()
            .find(|(t, _)| t.eq_ignore_ascii_case(k))
            .map(|(_, s)| *s)
            .unwrap_or(0)
    };
    let build = get("Build Cache");
    let images = get("Images");
    let containers = get("Containers");
    let mut out = Vec::new();
    if build > 0 {
        out.push(
            Candidate::new(
                format!("{label} build cache"),
                "Prunes the BuildKit layer cache only. Images and containers are untouched.",
                Category::Container,
                Risk::Low,
                Action::RunCommand {
                    program: program.into(),
                    args: vec!["builder".into(), "prune".into(), "-af".into()],
                    cwd: None,
                },
            )
            .with_size(build, false),
        );
    }
    if images + containers > 0 {
        out.push(
            Candidate::new(
                format!("{label} unused images & stopped containers"),
                "Removes dangling/unused images, stopped containers and unused networks. Volumes are kept.",
                Category::Container,
                Risk::Medium,
                Action::RunCommand { program: program.into(), args: vec!["system".into(), "prune".into(), "-af".into()], cwd: None },
            )
            .with_size(images + containers, false),
        );
    }
    out
}

fn pnpm_store() -> Vec<Candidate> {
    let Some(path) = tool_output("pnpm", &["store", "path"]) else {
        return Vec::new();
    };
    let path = std::path::PathBuf::from(path.trim());
    if !path.is_dir() {
        return Vec::new();
    }
    let size = dir_size(&path);
    if size == 0 {
        return Vec::new();
    }
    vec![Candidate::new(
        "pnpm store prune",
        format!("Removes packages no longer referenced by any project from {}. Store size is an upper bound.", path.display()),
        Category::PackageCache,
        Risk::Low,
        Action::RunCommand { program: "pnpm".into(), args: vec!["store".into(), "prune".into()], cwd: None },
    )
    .with_size(size / 4, false)]
}

/// Parse `xcrun simctl runtime list` lines into (name, size, id).
pub fn parse_simctl_runtimes(text: &str) -> Vec<(String, u64, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.contains(" - ") || line.starts_with("==") || line.starts_with("--") {
            continue;
        }
        let parts: Vec<&str> = line.split(" - ").map(|p| p.trim()).collect();
        if parts.len() < 3 {
            continue;
        }
        let name = parts[0].to_string();
        let size = parts
            .iter()
            .skip(1)
            .find_map(|p| parse_human_size(p))
            .unwrap_or(0);
        let Some(id) = parts.last().and_then(|p| p.split_whitespace().last()) else {
            continue;
        };
        if id.len() < 8 {
            continue;
        }
        out.push((name, size, id.to_string()));
    }
    out
}

fn simulator_runtimes() -> Vec<Candidate> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let Some(text) = tool_output("xcrun", &["simctl", "runtime", "list"]) else {
        return Vec::new();
    };
    parse_simctl_runtimes(&text)
        .into_iter()
        .map(|(name, size, id)| {
            Candidate::new(
                format!("Simulator runtime {name}"),
                "Deletes the simulator runtime disk image. Re-download from Xcode › Settings › Platforms.",
                Category::Mobile,
                Risk::Medium,
                Action::RunCommand { program: "xcrun".into(), args: vec!["simctl".into(), "runtime".into(), "delete".into(), id], cwd: None },
            )
            .with_size(size, size > 0)
        })
        .collect()
}

fn conda() -> Vec<Candidate> {
    if which::which("conda").is_err() {
        return Vec::new();
    }
    vec![Candidate::new(
        "conda clean",
        "Removes unused conda packages, tarballs and index caches.",
        Category::PackageCache,
        Risk::Low,
        Action::RunCommand {
            program: "conda".into(),
            args: vec!["clean".into(), "--all".into(), "--yes".into()],
            cwd: None,
        },
    )
    .with_size(0, false)]
}

/// Parse `docker images --format "{{.ID}}\t{{.Repository}}\t{{.Tag}}\t{{.Size}}\t{{.CreatedSince}}"`.
pub fn parse_images(text: &str) -> Vec<(String, String, u64, String)> {
    text.lines()
        .filter_map(|l| {
            let p: Vec<&str> = l.split('\t').collect();
            if p.len() < 5 {
                return None;
            }
            let name = if p[1] == "<none>" {
                format!("<untagged {}>", p[0])
            } else {
                format!("{}:{}", p[1], p[2])
            };
            Some((
                p[0].to_string(),
                name,
                parse_human_size(p[3]).unwrap_or(0),
                p[4].to_string(),
            ))
        })
        .collect()
}

/// Images not referenced by any container, one candidate each.
fn container_images(program: &str, label: &str) -> Vec<Candidate> {
    let Some(images) = tool_output(
        program,
        &[
            "images",
            "--format",
            "{{.ID}}\t{{.Repository}}\t{{.Tag}}\t{{.Size}}\t{{.CreatedSince}}",
        ],
    ) else {
        return Vec::new();
    };
    let used: std::collections::HashSet<String> =
        tool_output(program, &["ps", "-a", "--format", "{{.Image}}"])
            .map(|s| s.lines().map(|l| l.trim().to_string()).collect())
            .unwrap_or_default();
    parse_images(&images)
        .into_iter()
        .filter(|(id, name, size, _)| *size > 0 && !used.contains(name) && !used.iter().any(|u| id.starts_with(u) || u.starts_with(id)))
        .map(|(id, name, size, age)| {
            Candidate::new(
                format!("{label} image {name}"),
                format!("Image created {age}, not used by any container. Removed with `{program} rmi`; pull again when needed."),
                Category::Container,
                Risk::Medium,
                Action::RunCommand { program: program.into(), args: vec!["rmi".into(), id], cwd: None },
            )
            .with_size(size, false)
            .with_owner(label)
            .with_tag("image")
        })
        .collect()
}

/// Parse `xcrun simctl list devices --json` into (name, runtime, udid, state, dataPath).
pub fn parse_simctl_devices(json: &str) -> Vec<(String, String, String, String, String)> {
    let v: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    if let Some(map) = v.get("devices").and_then(|d| d.as_object()) {
        for (runtime, devices) in map {
            let rt = runtime
                .rsplit('.')
                .next()
                .unwrap_or(runtime)
                .replace('-', " ");
            if let Some(list) = devices.as_array() {
                for d in list {
                    let get = |k: &str| d.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
                    out.push((
                        get("name"),
                        rt.clone(),
                        get("udid"),
                        get("state"),
                        get("dataPath"),
                    ));
                }
            }
        }
    }
    out
}

fn simulator_devices() -> Vec<Candidate> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let Some(json) = tool_output("xcrun", &["simctl", "list", "devices", "--json"]) else {
        return Vec::new();
    };
    parse_simctl_devices(&json)
        .into_iter()
        .filter(|(_, _, _, state, data)| state != "Booted" && !data.is_empty())
        .filter_map(|(name, runtime, udid, _, data)| {
            let path = std::path::PathBuf::from(&data);
            let size = dir_size(&path);
            if size < 64 * 1024 * 1024 {
                return None;
            }
            Some(
                Candidate::new(
                    format!("Simulator device {name} ({runtime})"),
                    "Deletes this simulator and its app data. Create a new one in Xcode when needed.",
                    Category::Mobile,
                    Risk::Medium,
                    Action::RunCommand { program: "xcrun".into(), args: vec!["simctl".into(), "delete".into(), udid], cwd: None },
                )
                .with_size(size, true)
                .with_modified(crate::core::fsutil::modified_at(&path))
                .with_owner("Xcode")
                .with_tag("simulator"),
            )
        })
        .collect()
}

/// All tool-backed candidates available on this machine.
pub fn tool_candidates() -> Vec<Candidate> {
    let mut out = Vec::new();
    out.extend(container_engine("docker", "Docker"));
    out.extend(container_images("docker", "Docker"));
    out.extend(container_engine("podman", "Podman"));
    out.extend(container_images("podman", "Podman"));
    out.extend(pnpm_store());
    out.extend(simulator_runtimes());
    out.extend(simulator_devices());
    out.extend(conda());
    out
}

/// Which optional tools are present (for `doctor`).
pub fn tool_report() -> Vec<(&'static str, bool)> {
    ["git", "docker", "podman", "pnpm", "xcrun", "conda", "brew"]
        .into_iter()
        .map(|t| (t, which::which(t).is_ok()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docker_df() {
        let rows = parse_system_df("Images\t1.2GB (40%)\nContainers\t0B (0%)\nLocal Volumes\t512MB (100%)\nBuild Cache\t3.5GB\n");
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].0, "Images");
        assert_eq!(rows[3].1, (3.5 * 1024.0 * 1024.0 * 1024.0) as u64);
    }

    #[test]
    fn parses_images_and_devices() {
        let rows = parse_images("abc123\tnginx\t1.25\t187MB\t3 weeks ago\ndef456\t<none>\t<none>\t1.2GB\t2 months ago\n");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1, "nginx:1.25");
        assert_eq!(rows[1].1, "<untagged def456>");
        let json = r#"{"devices":{"com.apple.CoreSimulator.SimRuntime.iOS-17-5":[{"name":"iPhone 15","udid":"U1","state":"Shutdown","dataPath":"/tmp/x"}]}}"#;
        let d = parse_simctl_devices(json);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].1, "iOS 17 5");
    }

    #[test]
    fn parses_simctl() {
        let text = "== Disk Images ==\n-- iOS --\niOS 17.5 (21F79) - 7.65G - Ready - 1234ABCD-1111-2222-3333-444455556666\niOS 18.0 (22A3362) - 8.1G - Ready - FFFF0000-1111-2222-3333-444455556666\n";
        let rows = parse_simctl_runtimes(text);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "iOS 17.5 (21F79)");
        assert_eq!(rows[0].2, "1234ABCD-1111-2222-3333-444455556666");
        assert!(rows[0].1 > 7 * 1024 * 1024 * 1024);
    }

    #[test]
    fn timeout_kills_long_commands() {
        if which::which("sleep").is_err() {
            return;
        }
        let mut cmd = Command::new("sleep");
        cmd.arg("5");
        let start = Instant::now();
        assert!(run_with_timeout(cmd, Duration::from_millis(200)).is_none());
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
