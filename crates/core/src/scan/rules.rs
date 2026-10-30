//! Declarative table of global cache locations per operating system.
//!
//! Path templates:
//! - `~/…`        home directory
//! - `$CACHE/…`   OS cache dir (macOS `~/Library/Caches`, Linux `$XDG_CACHE_HOME`/`~/.cache`, Windows `%LOCALAPPDATA%`)
//! - `$DATA/…`    OS data dir (macOS `~/Library/Application Support`, Linux `~/.local/share`, Windows `%APPDATA%`)
//! - `$LOCALAPPDATA/…`, `$APPDATA/…`, `$TEMP/…` Windows environment folders

use std::path::{Path, PathBuf};

use crate::core::{Category, Risk};

pub const MAC: u8 = 1;
pub const LINUX: u8 = 2;
pub const WIN: u8 = 4;
pub const UNIX: u8 = MAC | LINUX;
pub const ALL: u8 = MAC | LINUX | WIN;

#[derive(Clone, Copy, Debug)]
pub struct Rule {
    pub label: &'static str,
    pub path: &'static str,
    pub category: Category,
    pub risk: Risk,
    pub description: &'static str,
    pub os: u8,
}

const fn r(
    label: &'static str,
    path: &'static str,
    category: Category,
    risk: Risk,
    description: &'static str,
    os: u8,
) -> Rule {
    Rule {
        label,
        path,
        category,
        risk,
        description,
        os,
    }
}

use Category::*;
use Risk::*;

pub static RULES: &[Rule] = &[
    // ── Package manager stores ──────────────────────────────────────────────
    r("npm cache", "~/.npm/_cacache", PackageCache, Low, "npm download cache. Re-downloaded on install.", UNIX),
    r("npm cache", "$LOCALAPPDATA/npm-cache", PackageCache, Low, "npm download cache. Re-downloaded on install.", WIN),
    r("Yarn cache", "$CACHE/Yarn", PackageCache, Low, "Yarn classic / berry global cache.", MAC),
    r("Yarn cache", "~/.cache/yarn", PackageCache, Low, "Yarn classic / berry global cache.", LINUX),
    r("Yarn cache", "$LOCALAPPDATA/Yarn/Cache", PackageCache, Low, "Yarn classic / berry global cache.", WIN),
    r("pnpm store", "~/Library/pnpm/store", PackageCache, Medium, "pnpm content-addressable store. Every project re-downloads after removal; prefer `pnpm store prune`.", MAC),
    r("pnpm store", "~/.local/share/pnpm/store", PackageCache, Medium, "pnpm content-addressable store. Prefer `pnpm store prune`.", LINUX),
    r("pnpm store", "$LOCALAPPDATA/pnpm/store", PackageCache, Medium, "pnpm content-addressable store. Prefer `pnpm store prune`.", WIN),
    r("pnpm cache", "$CACHE/pnpm", PackageCache, Low, "pnpm metadata cache.", ALL),
    r("Bun install cache", "~/.bun/install/cache", PackageCache, Low, "Bun package download cache.", UNIX),
    r("Deno cache", "$CACHE/deno", PackageCache, Low, "Deno module and npm cache.", ALL),
    r("Cargo registry cache", "~/.cargo/registry/cache", PackageCache, Low, "Downloaded crate archives. Re-fetched on build.", ALL),
    r("Cargo registry sources", "~/.cargo/registry/src", PackageCache, Low, "Extracted crate sources. Re-extracted on build.", ALL),
    r("Cargo git checkouts", "~/.cargo/git/checkouts", PackageCache, Low, "Checked-out git dependencies.", ALL),
    r("Go module cache", "~/go/pkg/mod/cache/download", PackageCache, Low, "Go module download cache (zip + info).", ALL),
    r("pip cache", "$CACHE/pip", PackageCache, Low, "pip wheel and HTTP cache.", ALL),
    r("uv cache", "~/.cache/uv", PackageCache, Low, "uv package cache.", UNIX),
    r("uv cache", "$LOCALAPPDATA/uv/cache", PackageCache, Low, "uv package cache.", WIN),
    r("Poetry cache", "$CACHE/pypoetry", PackageCache, Low, "Poetry artifact and PyPI cache.", ALL),
    r("conda packages", "~/miniconda3/pkgs", PackageCache, Medium, "Conda package cache. Environments keep working; reinstalls re-download.", UNIX),
    r("conda packages", "~/anaconda3/pkgs", PackageCache, Medium, "Conda package cache.", UNIX),
    r("Gradle caches", "~/.gradle/caches", PackageCache, Medium, "Gradle dependency and build caches. First build afterwards is slow.", ALL),
    r("Gradle wrapper distributions", "~/.gradle/wrapper/dists", PackageCache, Medium, "Downloaded Gradle distributions.", ALL),
    r("Gradle daemon logs", "~/.gradle/daemon", Temp, Low, "Gradle daemon logs and registry.", ALL),
    r("Maven repository", "~/.m2/repository", PackageCache, Medium, "Local Maven artifact repository.", ALL),
    r("CocoaPods cache", "$CACHE/CocoaPods", PackageCache, Low, "CocoaPods download cache.", MAC),
    r("CocoaPods specs", "~/.cocoapods/repos", PackageCache, Medium, "CocoaPods spec repositories (large git clones).", MAC),
    r("Dart pub cache", "~/.pub-cache", PackageCache, Medium, "Dart / Flutter package cache. Run `flutter pub get` afterwards.", UNIX),
    r("Dart pub cache", "$LOCALAPPDATA/Pub/Cache", PackageCache, Medium, "Dart / Flutter package cache.", WIN),
    r("Composer cache", "$CACHE/composer", PackageCache, Low, "Composer package cache.", UNIX),
    r("Composer cache", "~/.composer/cache", PackageCache, Low, "Composer package cache.", UNIX),
    r("Composer cache", "$LOCALAPPDATA/Composer", PackageCache, Low, "Composer package cache.", WIN),
    r("NuGet packages", "~/.nuget/packages", PackageCache, Medium, "NuGet global packages folder.", ALL),
    r("NuGet HTTP cache", "~/.local/share/NuGet/v3-cache", PackageCache, Low, "NuGet HTTP cache.", UNIX),
    r("NuGet HTTP cache", "$LOCALAPPDATA/NuGet/v3-cache", PackageCache, Low, "NuGet HTTP cache.", WIN),
    r("RubyGems cache", "~/.gem/ruby", PackageCache, Low, "Downloaded gem archives.", UNIX),
    r("Hex / Mix cache", "~/.hex", PackageCache, Low, "Elixir Hex package cache.", UNIX),
    r("Homebrew cache", "$CACHE/Homebrew", PackageCache, Low, "Downloaded bottles and casks.", MAC),
    r("Homebrew cache", "~/.cache/Homebrew", PackageCache, Low, "Downloaded bottles.", LINUX),
    r("rustup downloads", "~/.rustup/downloads", Temp, Low, "Toolchain download leftovers.", ALL),
    r("rustup tmp", "~/.rustup/tmp", Temp, Low, "Toolchain install leftovers.", ALL),
    r("Hugging Face hub cache", "$CACHE/huggingface/hub", PackageCache, Medium, "Downloaded models and datasets.", UNIX),
    r("Hugging Face hub cache", "~/.cache/huggingface/hub", PackageCache, Medium, "Downloaded models and datasets.", UNIX),
    r("Ollama models", "~/.ollama/models", AppData, High, "Local LLM weights. Re-pull with `ollama pull`.", UNIX),
    // ── Build caches ────────────────────────────────────────────────────────
    r("Go build cache", "$CACHE/go-build", BuildCache, Low, "Go compiler cache.", ALL),
    r("sccache", "$CACHE/sccache", BuildCache, Low, "Shared compilation cache.", UNIX),
    r("sccache", "~/.cache/sccache", BuildCache, Low, "Shared compilation cache.", LINUX),
    r("ccache", "~/.ccache", BuildCache, Low, "C/C++ compiler cache.", UNIX),
    r("ccache", "~/.cache/ccache", BuildCache, Low, "C/C++ compiler cache.", LINUX),
    r("Bazel cache", "~/.cache/bazel", BuildCache, Medium, "Bazel output base.", LINUX),
    r("Bazel cache", "/private/var/tmp/_bazel_$USER", BuildCache, Medium, "Bazel output base.", MAC),
    r("Turbo cache", "$CACHE/turbo", BuildCache, Low, "Turborepo remote cache mirror.", ALL),
    r("Nx cache", "$CACHE/nx", BuildCache, Low, "Nx task cache.", ALL),
    r("Vite / esbuild cache", "$CACHE/vite", BuildCache, Low, "Vite dependency pre-bundle cache.", ALL),
    r("Babel cache", "~/.babel.json", BuildCache, Low, "Babel register cache.", UNIX),
    r("Xcode DerivedData", "~/Library/Developer/Xcode/DerivedData", BuildCache, Low, "Xcode build output and indexes. Rebuilt on next build.", MAC),
    r("Xcode Archives", "~/Library/Developer/Xcode/Archives", AppData, High, "App archives used for App Store submissions and dSYM symbolication.", MAC),
    r("Xcode device logs", "~/Library/Developer/Xcode/DeviceLogs", Temp, Low, "Crash and device logs collected by Xcode.", MAC),
    r("Xcode iOS DeviceSupport", "~/Library/Developer/Xcode/iOS DeviceSupport", Mobile, Medium, "Debug symbols for connected iOS versions. Re-downloaded when a device connects.", MAC),
    r("Xcode watchOS DeviceSupport", "~/Library/Developer/Xcode/watchOS DeviceSupport", Mobile, Medium, "Debug symbols for watchOS devices.", MAC),
    r("Xcode Products", "~/Library/Developer/Xcode/Products", BuildCache, Low, "Installed product metadata cache.", MAC),
    r("Xcode cache", "$CACHE/com.apple.dt.Xcode", BuildCache, Low, "Xcode application cache.", MAC),
    r("Simulator caches", "~/Library/Developer/CoreSimulator/Caches", Mobile, Low, "iOS Simulator dyld caches.", MAC),
    r("Simulator devices", "~/Library/Developer/CoreSimulator/Devices", Mobile, High, "All simulator devices and their app data. Recreate in Xcode.", MAC),
    r("Android AVDs", "~/.android/avd", Mobile, High, "Android emulator images and their state.", ALL),
    r("Android build cache", "~/.android/build-cache", Mobile, Low, "Android Gradle plugin build cache.", ALL),
    r("Android SDK", "~/Library/Android/sdk", Mobile, High, "Android SDK, NDK and system images. Reinstall via Android Studio.", MAC),
    r("Android SDK", "~/Android/Sdk", Mobile, High, "Android SDK, NDK and system images.", LINUX),
    r("Android SDK", "$LOCALAPPDATA/Android/Sdk", Mobile, High, "Android SDK, NDK and system images.", WIN),
    r("Unity cache", "$CACHE/com.unity3d.UnityEditor", BuildCache, Low, "Unity editor cache.", MAC),
    r("Unity cache", "~/.cache/unity3d", BuildCache, Low, "Unity editor cache.", LINUX),
    // ── IDEs and editors ────────────────────────────────────────────────────
    r("JetBrains caches", "$CACHE/JetBrains", Ide, Low, "IntelliJ / WebStorm / PyCharm index caches. Projects re-index.", ALL),
    r("JetBrains logs", "~/Library/Logs/JetBrains", Temp, Low, "IDE log files.", MAC),
    r("VS Code cache", "$DATA/Code/Cache", Ide, Low, "VS Code HTTP cache.", ALL),
    r("VS Code cached data", "$DATA/Code/CachedData", Ide, Low, "VS Code V8 cached data.", ALL),
    r("VS Code extension downloads", "$DATA/Code/CachedExtensionVSIXs", Ide, Low, "Downloaded extension packages.", ALL),
    r("VS Code workspace storage", "$DATA/Code/User/workspaceStorage", Ide, Medium, "Per-workspace state (unsaved editors, extension state).", ALL),
    r("Cursor cache", "$DATA/Cursor/Cache", Ide, Low, "Cursor HTTP cache.", ALL),
    r("Cursor cached data", "$DATA/Cursor/CachedData", Ide, Low, "Cursor V8 cached data.", ALL),
    r("Cursor extension downloads", "$DATA/Cursor/CachedExtensionVSIXs", Ide, Low, "Downloaded extension packages.", ALL),
    r("Cursor workspace storage", "$DATA/Cursor/User/workspaceStorage", Ide, Medium, "Per-workspace state.", ALL),
    r("Cursor global state DB", "$DATA/Cursor/User/globalStorage/state.vscdb", AppData, High, "Cursor global state database. Close Cursor first; settings and chat state may reset.", ALL),
    r("Windsurf cache", "$DATA/Windsurf/Cache", Ide, Low, "Windsurf HTTP cache.", ALL),
    r("Windsurf cached data", "$DATA/Windsurf/CachedData", Ide, Low, "Windsurf V8 cached data.", ALL),
    r("Zed cache", "$CACHE/Zed", Ide, Low, "Zed editor cache.", MAC),
    r("Android Studio caches", "$CACHE/Google", Ide, Low, "Android Studio index caches (and Chrome caches on macOS).", MAC),
    r("Android Studio caches", "~/.cache/Google", Ide, Low, "Android Studio index caches.", LINUX),
    r("Claude VM bundles", "$DATA/Claude/vm_bundles", AppData, High, "Claude Desktop local VM bundle. Close Claude first; re-downloaded on demand.", ALL),
    r("Claude Desktop cache", "$DATA/Claude/Cache", AppData, Low, "Claude Desktop HTTP cache.", ALL),
    r("Codex runtime cache", "~/.cache/codex-runtimes", Ide, Medium, "Bundled runtime dependencies.", UNIX),
    r("Copilot CLI cache", "~/.copilot/cache", Ide, Low, "GitHub Copilot CLI cache.", UNIX),
    // ── Browsers ────────────────────────────────────────────────────────────
    r("Chrome cache", "~/.cache/google-chrome", Browser, Low, "Google Chrome cache.", LINUX),
    r("Chrome cache", "$LOCALAPPDATA/Google/Chrome/User Data/Default/Cache", Browser, Low, "Google Chrome cache.", WIN),
    r("Chrome code cache", "$LOCALAPPDATA/Google/Chrome/User Data/Default/Code Cache", Browser, Low, "Google Chrome code cache.", WIN),
    r("Chrome on-device model", "$DATA/Google/Chrome/OptGuideOnDeviceModel", Browser, Low, "Chrome local AI model cache.", ALL),
    r("Chromium cache", "~/.cache/chromium", Browser, Low, "Chromium cache.", LINUX),
    r("Brave cache", "$CACHE/BraveSoftware", Browser, Low, "Brave browser cache.", MAC),
    r("Brave cache", "~/.cache/BraveSoftware", Browser, Low, "Brave browser cache.", LINUX),
    r("Arc cache", "$CACHE/company.thebrowser.Browser", Browser, Low, "Arc browser cache.", MAC),
    r("Firefox cache", "$CACHE/Firefox", Browser, Low, "Firefox profile caches.", MAC),
    r("Firefox cache", "~/.cache/mozilla/firefox", Browser, Low, "Firefox profile caches.", LINUX),
    r("Firefox cache", "$LOCALAPPDATA/Mozilla/Firefox/Profiles", Browser, Low, "Firefox profile caches.", WIN),
    r("Edge cache", "$LOCALAPPDATA/Microsoft/Edge/User Data/Default/Cache", Browser, Low, "Microsoft Edge cache.", WIN),
    r("Edge cache", "$CACHE/Microsoft Edge", Browser, Low, "Microsoft Edge cache.", MAC),
    r("Safari cache", "$CACHE/com.apple.Safari", Browser, Low, "Safari cache.", MAC),
    // ── Chat / media apps ───────────────────────────────────────────────────
    r("Slack cache", "$DATA/Slack/Cache", AppData, Low, "Slack HTTP cache.", ALL),
    r("Slack service worker cache", "$DATA/Slack/Service Worker/CacheStorage", AppData, Low, "Slack service worker cache.", ALL),
    r("Discord cache", "$DATA/discord/Cache", AppData, Low, "Discord HTTP cache.", ALL),
    r("Discord code cache", "$DATA/discord/Code Cache", AppData, Low, "Discord code cache.", ALL),
    r("Teams cache", "$DATA/Microsoft/Teams/Cache", AppData, Low, "Microsoft Teams cache.", ALL),
    r("Spotify cache", "$CACHE/com.spotify.client", AppData, Low, "Spotify media cache.", MAC),
    r("Spotify cache", "~/.cache/spotify", AppData, Low, "Spotify media cache.", LINUX),
    r("Spotify cache", "$LOCALAPPDATA/Spotify/Storage", AppData, Low, "Spotify media cache.", WIN),
    r("Notion cache", "$DATA/Notion/Cache", AppData, Low, "Notion HTTP cache.", ALL),
    r("Figma cache", "$DATA/Figma/Cache", AppData, Low, "Figma desktop cache.", ALL),
    r("Postman cache", "$DATA/Postman/Cache", AppData, Low, "Postman HTTP cache.", ALL),
    // ── Containers / VMs ────────────────────────────────────────────────────
    r("Docker Desktop logs", "~/Library/Containers/com.docker.docker/Data/log", Temp, Low, "Docker Desktop log files.", MAC),
    r("Vagrant boxes", "~/.vagrant.d/boxes", Container, High, "Downloaded Vagrant base boxes.", ALL),
    r("Minikube cache", "~/.minikube/cache", Container, Medium, "Minikube ISO and image cache.", ALL),
    r("Colima cache", "$CACHE/colima", Container, Medium, "Colima VM image cache.", MAC),
    r("Lima cache", "$CACHE/lima", Container, Medium, "Lima VM image cache.", MAC),
    // ── Temp and trash ──────────────────────────────────────────────────────
    r("User trash", "~/.Trash", Temp, Medium, "Files already in the trash. Emptying is irreversible.", MAC),
    r("User trash", "~/.local/share/Trash/files", Temp, Medium, "Files already in the trash. Emptying is irreversible.", LINUX),
    r("User logs", "~/Library/Logs", Temp, Low, "Application log files.", MAC),
    r("Windows temp", "$TEMP", Temp, Medium, "User temporary files. Close running installers first.", WIN),
    r("Windows crash dumps", "$LOCALAPPDATA/CrashDumps", Temp, Low, "Application crash dumps.", WIN),
    // ── AI tooling ──────────────────────────────────────────────────────────
    r("LM Studio models", "~/.lmstudio/models", AppData, High, "Local LLM weights downloaded by LM Studio.", ALL),
    r("LM Studio models", "$CACHE/lm-studio/models", AppData, High, "Local LLM weights downloaded by LM Studio.", ALL),
    r("Ollama models", "$LOCALAPPDATA/Ollama/models", AppData, High, "Local LLM weights. Re-pull with `ollama pull`.", WIN),
    r("Hugging Face hub cache", "$LOCALAPPDATA/huggingface/hub", PackageCache, Medium, "Downloaded models and datasets.", WIN),
    r("Claude Code debug logs", "~/.claude/debug", Temp, Low, "Claude Code session debug logs.", ALL),
    r("Codex logs", "~/.codex/log", Temp, Low, "Codex CLI logs.", ALL),
    r("Playwright browsers", "$CACHE/ms-playwright", PackageCache, Medium, "Browser builds downloaded by Playwright. Re-run `npx playwright install`.", ALL),
    r("Puppeteer browsers", "$CACHE/puppeteer", PackageCache, Medium, "Chromium builds downloaded by Puppeteer.", ALL),
    r("Cypress binaries", "$CACHE/Cypress", PackageCache, Medium, "Cypress app binaries per version.", ALL),
    r("Electron downloads", "$CACHE/electron", PackageCache, Low, "Electron zip cache used by electron-builder.", ALL),
    r("node-gyp headers", "$CACHE/node-gyp", PackageCache, Low, "Node headers for native module builds.", ALL),
    r("Jupyter runtime", "~/Library/Jupyter/runtime", Temp, Low, "Kernel connection files.", MAC),
    r("Xcode previews", "~/Library/Developer/Xcode/UserData/Previews", BuildCache, Low, "SwiftUI preview build cache.", MAC),
    r("Carthage cache", "$CACHE/org.carthage.CarthageKit", PackageCache, Low, "Carthage dependency cache.", MAC),
    r("SwiftPM cache", "$CACHE/org.swift.swiftpm", PackageCache, Low, "Swift Package Manager repository cache.", MAC),
    r("Flutter engine artifacts", "~/flutter/bin/cache", BuildCache, Medium, "Flutter SDK engine artifacts. Re-downloaded by `flutter precache`.", UNIX),
    r("Terraform plugin cache", "~/.terraform.d/plugin-cache", PackageCache, Low, "Provider binaries cache.", ALL),
    r("Helm cache", "$CACHE/helm", PackageCache, Low, "Helm chart repository cache.", ALL),
    r("kubectl cache", "~/.kube/cache", Temp, Low, "kubectl discovery cache.", ALL),
    r("Vercel CLI cache", "$DATA/com.vercel.cli", Temp, Low, "Vercel CLI cache.", ALL),
    r("Gradle build-scan", "~/.gradle/build-scan-data", Temp, Low, "Build scan uploads.", ALL),
    r("Android Studio logs", "~/Library/Logs/Google", Temp, Low, "Android Studio log files.", MAC),
    r("Windows delivery optimization", "$LOCALAPPDATA/Microsoft/Windows/DeliveryOptimization", Temp, Low, "Windows Update download cache.", WIN),
    r("Windows thumbnail cache", "$LOCALAPPDATA/Microsoft/Windows/Explorer", Temp, Low, "Explorer thumbnail databases.", WIN),
];

/// Owning application (`owner`) and the process names that lock it (`app`), by label prefix.
/// Process names are `|` separated aliases; an empty `app` means cleaning is safe while running.
pub static OWNERS: &[(&str, &str, &str)] = &[
    ("Xcode", "Xcode", ""),
    (
        "Simulator devices",
        "Xcode",
        "Simulator|CoreSimulatorService",
    ),
    ("Simulator", "Xcode", ""),
    (
        "Android",
        "Android Studio",
        "qemu-system-x86_64|qemu-system-aarch64|emulator",
    ),
    ("Cursor global", "Cursor", "Cursor"),
    ("Cursor workspace", "Cursor", "Cursor"),
    ("Cursor", "Cursor", ""),
    ("VS Code workspace", "VS Code", "Code|Visual Studio Code"),
    ("VS Code", "VS Code", ""),
    ("Windsurf", "Windsurf", ""),
    ("Zed", "Zed", ""),
    ("JetBrains", "JetBrains", ""),
    ("Claude VM", "Claude", "Claude"),
    ("Claude", "Claude", ""),
    ("Codex", "Codex", ""),
    ("Copilot", "Copilot", ""),
    ("Chrome", "Chrome", "Google Chrome|chrome"),
    ("Chromium", "Chromium", "chromium"),
    ("Brave", "Brave", "Brave Browser|brave"),
    ("Arc", "Arc", "Arc"),
    ("Firefox", "Firefox", "firefox"),
    ("Edge", "Edge", "Microsoft Edge|msedge"),
    ("Safari", "Safari", "Safari"),
    ("Slack", "Slack", "Slack"),
    ("Discord", "Discord", "Discord"),
    ("Teams", "Teams", "Teams|MSTeams"),
    ("Spotify", "Spotify", "Spotify"),
    ("Notion", "Notion", "Notion"),
    ("Figma", "Figma", "Figma"),
    ("Postman", "Postman", "Postman"),
    ("Ollama", "Ollama", "ollama"),
    ("LM Studio", "LM Studio", "LM Studio"),
    ("Hugging Face", "Hugging Face", ""),
    ("Docker", "Docker", ""),
    ("Vagrant", "Vagrant", ""),
    ("Minikube", "Minikube", ""),
    ("Colima", "Colima", ""),
    ("Lima", "Lima", ""),
    ("npm", "npm", ""),
    ("Yarn", "Yarn", ""),
    ("pnpm", "pnpm", ""),
    ("Bun", "Bun", ""),
    ("Deno", "Deno", ""),
    ("Cargo", "Cargo", ""),
    ("rustup", "rustup", ""),
    ("Go ", "Go", ""),
    ("pip", "pip", ""),
    ("uv", "uv", ""),
    ("Poetry", "Poetry", ""),
    ("conda", "conda", ""),
    ("Gradle", "Gradle", ""),
    ("Maven", "Maven", ""),
    ("CocoaPods", "CocoaPods", ""),
    ("Dart", "Dart", ""),
    ("Flutter", "Flutter", ""),
    ("Composer", "Composer", ""),
    ("NuGet", "NuGet", ""),
    ("RubyGems", "RubyGems", ""),
    ("Hex", "Hex", ""),
    ("Homebrew", "Homebrew", ""),
    ("Unity", "Unity", "Unity"),
    ("Playwright", "Playwright", ""),
    ("Puppeteer", "Puppeteer", ""),
    ("Cypress", "Cypress", ""),
    ("Electron", "Electron", ""),
    ("Bazel", "Bazel", ""),
    ("User trash", "System", ""),
    ("User logs", "System", ""),
    ("Windows", "Windows", ""),
];

/// Look up (owner, app) for a rule label.
pub fn owner_of(label: &str) -> (&'static str, &'static str) {
    OWNERS
        .iter()
        .find(|(prefix, _, _)| label.starts_with(prefix))
        .map(|(_, owner, app)| (*owner, *app))
        .unwrap_or(("", ""))
}

/// The current OS mask.
pub fn current_os() -> u8 {
    if cfg!(target_os = "macos") {
        MAC
    } else if cfg!(windows) {
        WIN
    } else {
        LINUX
    }
}

/// Resolve a rule path template to an absolute path for this machine.
pub fn resolve_template(template: &str, home: &Path) -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let cache_dir = || -> PathBuf {
        if cfg!(target_os = "macos") {
            home.join("Library/Caches")
        } else if cfg!(windows) {
            env("LOCALAPPDATA").unwrap_or_else(|| home.join("AppData/Local"))
        } else {
            env("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache"))
        }
    };
    let data_dir = || -> PathBuf {
        if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else if cfg!(windows) {
            env("APPDATA").unwrap_or_else(|| home.join("AppData/Roaming"))
        } else {
            env("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"))
        }
    };
    let (prefix, rest) = match template.find('/') {
        Some(i) if template.starts_with('$') || template.starts_with('~') => {
            (&template[..i], &template[i + 1..])
        }
        _ if template.starts_with('$') => (template, ""),
        _ => ("", template),
    };
    let base = match prefix {
        "~" => home.to_path_buf(),
        "$CACHE" => cache_dir(),
        "$DATA" => data_dir(),
        "$LOCALAPPDATA" => env("LOCALAPPDATA")?,
        "$APPDATA" => env("APPDATA")?,
        "$TEMP" => env("TEMP").or_else(|| env("TMP"))?,
        "" => {
            let user = std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_default();
            return Some(PathBuf::from(template.replace("$USER", &user)));
        }
        _ => return None,
    };
    if rest.is_empty() {
        Some(base)
    } else {
        Some(base.join(rest))
    }
}

/// Rules that apply to the running OS.
pub fn rules_for_current_os() -> impl Iterator<Item = &'static Rule> {
    let os = current_os();
    RULES.iter().filter(move |r| r.os & os != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_resolve() {
        let home = Path::new("/home/me");
        assert_eq!(
            resolve_template("~/.npm/_cacache", home).unwrap(),
            PathBuf::from("/home/me/.npm/_cacache")
        );
        let cache = resolve_template("$CACHE/pip", home).unwrap();
        assert!(cache.ends_with("pip"));
        let data = resolve_template("$DATA/Code/Cache", home).unwrap();
        assert!(data.ends_with("Code/Cache"));
        assert!(resolve_template("$NOPE/x", home).is_none());
    }

    #[test]
    fn owners_resolve() {
        assert_eq!(owner_of("Cursor global state DB"), ("Cursor", "Cursor"));
        assert_eq!(owner_of("Cursor cache"), ("Cursor", ""));
        assert_eq!(owner_of("Xcode DerivedData").0, "Xcode");
        assert_eq!(owner_of("Something unknown"), ("", ""));
    }

    #[test]
    fn rule_table_has_no_duplicates_per_os() {
        for os in [MAC, LINUX, WIN] {
            let mut seen = std::collections::HashSet::new();
            for rule in RULES.iter().filter(|r| r.os & os != 0) {
                assert!(
                    seen.insert(rule.path),
                    "duplicate rule path {} for os {os}",
                    rule.path
                );
            }
        }
    }
}
