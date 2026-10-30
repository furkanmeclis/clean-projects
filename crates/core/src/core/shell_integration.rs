//! "Scan / Clean with CleanProjects" entries in the file manager's context menu.
//!
//! - macOS: Finder Quick Actions (Automator workflows in `~/Library/Services`).
//! - Windows: `HKCU\Software\Classes\Directory\shell` registry keys (per user, no admin).
//! - Linux: Nautilus scripts and a KDE/Dolphin service menu.
//!
//! Every entry launches the given executable with `--scan <dir>` or `--clean <dir>`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntegrationStatus {
    pub installed: bool,
    pub platform: String,
    /// Files or registry keys that make up the integration.
    pub locations: Vec<String>,
    pub note: String,
}

const ACTIONS: [(&str, &str, &str); 2] = [
    ("scan", "Scan with CleanProjects", "--scan"),
    ("clean", "Clean with CleanProjects", "--clean"),
];

/// Locations used on this platform, relative to `home`.
pub fn locations(home: &Path) -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        ACTIONS
            .iter()
            .map(|(_, title, _)| {
                home.join("Library/Services")
                    .join(format!("{title}.workflow"))
            })
            .collect()
    } else if cfg!(windows) {
        vec![]
    } else {
        let mut v: Vec<PathBuf> = ACTIONS
            .iter()
            .map(|(_, title, _)| home.join(".local/share/nautilus/scripts").join(title))
            .collect();
        v.push(home.join(".local/share/kio/servicemenus/cleanprojects.desktop"));
        v
    }
}

pub fn status(home: &Path) -> IntegrationStatus {
    if cfg!(windows) {
        let installed = windows_key_exists("Directory\\shell\\CleanProjects.scan");
        IntegrationStatus {
            installed,
            platform: "windows".into(),
            locations: vec![
                "HKCU\\Software\\Classes\\Directory\\shell\\CleanProjects.*".into(),
                "HKCU\\Software\\Classes\\Directory\\Background\\shell\\CleanProjects.*".into(),
            ],
            note: "Right-click a folder or the folder background in Explorer.".into(),
        }
    } else {
        let locs = locations(home);
        let installed = locs.iter().any(|l| l.exists());
        IntegrationStatus {
            installed,
            platform: if cfg!(target_os = "macos") {
                "macos".into()
            } else {
                "linux".into()
            },
            locations: locs.iter().map(|l| l.display().to_string()).collect(),
            note: if cfg!(target_os = "macos") {
                "Finder › right-click a folder › Quick Actions. Log out and in if the entries do not appear.".into()
            } else {
                "Nautilus/Files: right-click › Scripts. Dolphin: right-click › Actions.".into()
            },
        }
    }
}

/// Install the context-menu entries that launch `exe`.
pub fn install(home: &Path, exe: &Path) -> Result<IntegrationStatus> {
    if cfg!(target_os = "macos") {
        install_macos(home, exe)?;
    } else if cfg!(windows) {
        install_windows(exe)?;
    } else {
        install_linux(home, exe)?;
    }
    Ok(status(home))
}

pub fn uninstall(home: &Path) -> Result<IntegrationStatus> {
    if cfg!(windows) {
        for (id, _, _) in ACTIONS {
            for base in ["Directory\\shell", "Directory\\Background\\shell"] {
                let _ = std::process::Command::new("reg")
                    .args([
                        "delete",
                        &format!("HKCU\\Software\\Classes\\{base}\\CleanProjects.{id}"),
                        "/f",
                    ])
                    .output();
            }
        }
    } else {
        for l in locations(home) {
            if l.is_dir() {
                let _ = fs::remove_dir_all(&l);
            } else if l.exists() {
                let _ = fs::remove_file(&l);
            }
        }
        if cfg!(target_os = "macos") {
            let _ = std::process::Command::new("/System/Library/CoreServices/pbs")
                .arg("-update")
                .output();
        }
    }
    Ok(status(home))
}

fn shell_quote(p: &Path) -> String {
    format!("'{}'", p.display().to_string().replace('\'', "'\\''"))
}

fn install_macos(home: &Path, exe: &Path) -> Result<()> {
    for (id, title, flag) in ACTIONS {
        let dir = home
            .join("Library/Services")
            .join(format!("{title}.workflow"))
            .join("Contents");
        fs::create_dir_all(&dir)?;
        let script = format!(
            "for f in \"$@\"; do {} {flag} \"$f\" >/dev/null 2>&1 & done",
            shell_quote(exe)
        );
        fs::write(dir.join("Info.plist"), macos_info_plist(title))?;
        fs::write(dir.join("document.wflow"), macos_workflow(&script, id))?;
    }
    let _ = std::process::Command::new("/System/Library/CoreServices/pbs")
        .arg("-update")
        .output();
    Ok(())
}

fn macos_info_plist(title: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>NSServices</key>
	<array>
		<dict>
			<key>NSMenuItem</key>
			<dict>
				<key>default</key>
				<string>{title}</string>
			</dict>
			<key>NSMessage</key>
			<string>runWorkflowAsService</string>
			<key>NSRequiredContext</key>
			<dict>
				<key>NSApplicationIdentifier</key>
				<string>com.apple.finder</string>
			</dict>
			<key>NSSendFileTypes</key>
			<array>
				<string>public.folder</string>
			</array>
		</dict>
	</array>
</dict>
</plist>
"#
    )
}

fn macos_workflow(script: &str, id: &str) -> String {
    let esc = script
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    let uuid = |n: u8| {
        format!(
            "9E6B2C3A-0000-4000-8000-00000000{:02X}{}",
            n,
            if id == "scan" { "01" } else { "02" }
        )
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>AMApplicationBuild</key>
	<string>523</string>
	<key>AMApplicationVersion</key>
	<string>2.10</string>
	<key>AMDocumentVersion</key>
	<string>2</string>
	<key>actions</key>
	<array>
		<dict>
			<key>action</key>
			<dict>
				<key>AMAccepts</key>
				<dict>
					<key>Container</key>
					<string>List</string>
					<key>Optional</key>
					<true/>
					<key>Types</key>
					<array>
						<string>com.apple.cocoa.string</string>
					</array>
				</dict>
				<key>AMActionVersion</key>
				<string>2.0.3</string>
				<key>AMApplication</key>
				<array>
					<string>Automator</string>
				</array>
				<key>AMParameterProperties</key>
				<dict>
					<key>COMMAND_STRING</key>
					<dict/>
					<key>CheckedForUserDefaultShell</key>
					<dict/>
					<key>inputMethod</key>
					<dict/>
					<key>shell</key>
					<dict/>
					<key>source</key>
					<dict/>
				</dict>
				<key>AMProvides</key>
				<dict>
					<key>Container</key>
					<string>List</string>
					<key>Types</key>
					<array>
						<string>com.apple.cocoa.string</string>
					</array>
				</dict>
				<key>ActionBundlePath</key>
				<string>/System/Library/Automator/Run Shell Script.action</string>
				<key>ActionName</key>
				<string>Run Shell Script</string>
				<key>ActionParameters</key>
				<dict>
					<key>COMMAND_STRING</key>
					<string>{esc}</string>
					<key>CheckedForUserDefaultShell</key>
					<true/>
					<key>inputMethod</key>
					<integer>1</integer>
					<key>shell</key>
					<string>/bin/zsh</string>
					<key>source</key>
					<string></string>
				</dict>
				<key>BundleIdentifier</key>
				<string>com.apple.RunShellScript</string>
				<key>CFBundleVersion</key>
				<string>2.0.3</string>
				<key>CanShowSelectedItemsWhenRun</key>
				<false/>
				<key>CanShowWhenRun</key>
				<true/>
				<key>Category</key>
				<array>
					<string>AMCategoryUtilities</string>
				</array>
				<key>Class Name</key>
				<string>RunShellScriptAction</string>
				<key>InputUUID</key>
				<string>{in_uuid}</string>
				<key>Keywords</key>
				<array>
					<string>Shell</string>
					<string>Script</string>
					<string>Command</string>
					<string>Run</string>
					<string>Unix</string>
				</array>
				<key>OutputUUID</key>
				<string>{out_uuid}</string>
				<key>UUID</key>
				<string>{uuid}</string>
				<key>UnlocalizedApplications</key>
				<array>
					<string>Automator</string>
				</array>
				<key>arguments</key>
				<dict>
					<key>0</key>
					<dict>
						<key>default value</key>
						<integer>0</integer>
						<key>name</key>
						<string>inputMethod</string>
						<key>required</key>
						<string>0</string>
						<key>type</key>
						<string>0</string>
						<key>uuid</key>
						<string>0</string>
					</dict>
					<key>1</key>
					<dict>
						<key>default value</key>
						<false/>
						<key>name</key>
						<string>CheckedForUserDefaultShell</string>
						<key>required</key>
						<string>0</string>
						<key>type</key>
						<string>0</string>
						<key>uuid</key>
						<string>1</string>
					</dict>
					<key>2</key>
					<dict>
						<key>default value</key>
						<string></string>
						<key>name</key>
						<string>source</string>
						<key>required</key>
						<string>0</string>
						<key>type</key>
						<string>0</string>
						<key>uuid</key>
						<string>2</string>
					</dict>
					<key>3</key>
					<dict>
						<key>default value</key>
						<string></string>
						<key>name</key>
						<string>COMMAND_STRING</string>
						<key>required</key>
						<string>0</string>
						<key>type</key>
						<string>0</string>
						<key>uuid</key>
						<string>3</string>
					</dict>
					<key>4</key>
					<dict>
						<key>default value</key>
						<string>/bin/sh</string>
						<key>name</key>
						<string>shell</string>
						<key>required</key>
						<string>0</string>
						<key>type</key>
						<string>0</string>
						<key>uuid</key>
						<string>4</string>
					</dict>
				</dict>
				<key>isViewVisible</key>
				<integer>1</integer>
				<key>location</key>
				<string>309.000000:253.000000</string>
				<key>nibPath</key>
				<string>/System/Library/Automator/Run Shell Script.action/Contents/Resources/Base.lproj/main.nib</string>
			</dict>
			<key>isViewVisible</key>
			<integer>1</integer>
		</dict>
	</array>
	<key>connectors</key>
	<dict/>
	<key>workflowMetaData</key>
	<dict>
		<key>applicationBundleIDsByPath</key>
		<dict/>
		<key>applicationPaths</key>
		<array/>
		<key>inputTypeIdentifier</key>
		<string>com.apple.Automator.fileSystemObject.folder</string>
		<key>outputTypeIdentifier</key>
		<string>com.apple.Automator.nothing</string>
		<key>presentationMode</key>
		<integer>11</integer>
		<key>processesInput</key>
		<integer>0</integer>
		<key>serviceApplicationBundleID</key>
		<string>com.apple.finder</string>
		<key>serviceApplicationPath</key>
		<string>/System/Library/CoreServices/Finder.app</string>
		<key>serviceInputTypeIdentifier</key>
		<string>com.apple.Automator.fileSystemObject.folder</string>
		<key>serviceOutputTypeIdentifier</key>
		<string>com.apple.Automator.nothing</string>
		<key>serviceProcessesInput</key>
		<integer>0</integer>
		<key>systemImageName</key>
		<string>NSTouchBarDeleteTemplate</string>
		<key>useAutomaticInputType</key>
		<integer>0</integer>
		<key>workflowTypeIdentifier</key>
		<string>com.apple.Automator.servicesMenu</string>
	</dict>
</dict>
</plist>
"#,
        in_uuid = uuid(1),
        out_uuid = uuid(2),
        uuid = uuid(3),
    )
}

fn windows_key_exists(sub: &str) -> bool {
    std::process::Command::new("reg")
        .args(["query", &format!("HKCU\\Software\\Classes\\{sub}")])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn install_windows(exe: &Path) -> Result<()> {
    let exe_s = exe.display().to_string();
    for (id, title, flag) in ACTIONS {
        for (base, arg) in [
            ("Directory\\shell", "%1"),
            ("Directory\\Background\\shell", "%V"),
        ] {
            let key = format!("HKCU\\Software\\Classes\\{base}\\CleanProjects.{id}");
            run_reg(&["add", &key, "/ve", "/d", title, "/f"])?;
            run_reg(&[
                "add",
                &key,
                "/v",
                "Icon",
                "/d",
                &format!("\"{exe_s}\",0"),
                "/f",
            ])?;
            run_reg(&[
                "add",
                &format!("{key}\\command"),
                "/ve",
                "/d",
                &format!("\"{exe_s}\" {flag} \"{arg}\""),
                "/f",
            ])?;
        }
    }
    Ok(())
}

fn run_reg(args: &[&str]) -> Result<()> {
    let out = std::process::Command::new("reg")
        .args(args)
        .output()
        .context("running reg.exe")?;
    if !out.status.success() {
        anyhow::bail!(
            "reg {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

fn install_linux(home: &Path, exe: &Path) -> Result<()> {
    let scripts = home.join(".local/share/nautilus/scripts");
    fs::create_dir_all(&scripts)?;
    for (_, title, flag) in ACTIONS {
        let p = scripts.join(title);
        fs::write(
            &p,
            format!(
                "#!/bin/sh\n# Generated by CleanProjects. Nautilus passes selected items as arguments.\nif [ \"$#\" -eq 0 ]; then\n  dir=$(printf '%s' \"$NAUTILUS_SCRIPT_CURRENT_URI\" | sed 's|^file://||' | python3 -c 'import sys,urllib.parse;print(urllib.parse.unquote(sys.stdin.read().strip()))' 2>/dev/null)\n  set -- \"$dir\"\nfi\nfor f in \"$@\"; do {} {flag} \"$f\" >/dev/null 2>&1 &\ndone\n",
                shell_quote(exe)
            ),
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755))?;
        }
    }
    let kde = home.join(".local/share/kio/servicemenus");
    fs::create_dir_all(&kde)?;
    let exe_s = exe.display().to_string();
    fs::write(
        kde.join("cleanprojects.desktop"),
        format!(
            "[Desktop Entry]\nType=Service\nServiceTypes=KonqPopupMenu/Plugin\nMimeType=inode/directory;\nActions=cleanprojectsScan;cleanprojectsClean;\nX-KDE-Priority=TopLevel\n\n[Desktop Action cleanprojectsScan]\nName=Scan with CleanProjects\nIcon=edit-find\nExec=\"{exe_s}\" --scan %f\n\n[Desktop Action cleanprojectsClean]\nName=Clean with CleanProjects\nIcon=edit-clear\nExec=\"{exe_s}\" --clean %f\n"
        ),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            kde.join("cleanprojects.desktop"),
            fs::Permissions::from_mode(0o755),
        )?;
    }
    Ok(())
}

/// What the app was asked to do on launch: `--scan DIR` or `--clean DIR`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaunchAction {
    pub kind: String,
    pub path: String,
}

/// Parse `--scan`/`--clean` from an argument list (the first match wins).
pub fn parse_launch_args<I: IntoIterator<Item = String>>(args: I) -> Option<LaunchAction> {
    let args: Vec<String> = args.into_iter().collect();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        for (kind, flag) in [("scan", "--scan"), ("clean", "--clean")] {
            if a == flag {
                if let Some(p) = args.get(i + 1) {
                    return Some(LaunchAction {
                        kind: kind.into(),
                        path: p.clone(),
                    });
                }
            } else if let Some(p) = a.strip_prefix(&format!("{flag}=")) {
                return Some(LaunchAction {
                    kind: kind.into(),
                    path: p.to_string(),
                });
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_launch_args() {
        assert_eq!(
            parse_launch_args(["app".to_string(), "--scan".into(), "/x y".into()])
                .unwrap()
                .path,
            "/x y"
        );
        assert_eq!(
            parse_launch_args(["app".to_string(), "--clean=/z".into()])
                .unwrap()
                .kind,
            "clean"
        );
        assert!(parse_launch_args(["app".to_string(), "--background".into()]).is_none());
    }

    #[cfg(not(windows))]
    #[test]
    fn installs_and_removes_files() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_path_buf();
        assert!(!status(&home).installed);
        let st = install(
            &home,
            Path::new("/Applications/CleanProjects.app/Contents/MacOS/CleanProjects"),
        )
        .unwrap();
        assert!(st.installed);
        for l in locations(&home) {
            assert!(l.exists(), "{}", l.display());
        }
        if cfg!(target_os = "linux") {
            let script = fs::read_to_string(
                home.join(".local/share/nautilus/scripts/Scan with CleanProjects"),
            )
            .unwrap();
            assert!(script.contains("--scan"));
            let kde = fs::read_to_string(
                home.join(".local/share/kio/servicemenus/cleanprojects.desktop"),
            )
            .unwrap();
            assert!(kde.contains("--clean %f"));
        } else {
            let wf =
                fs::read_to_string(home.join(
                    "Library/Services/Scan with CleanProjects.workflow/Contents/document.wflow",
                ))
                .unwrap();
            assert!(wf.contains("--scan") && wf.contains("servicesMenu"));
        }
        let st = uninstall(&home).unwrap();
        assert!(!st.installed);
    }
}
