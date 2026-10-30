# Changelog

## 2.1.0

### Added
- Recovery bin: removed items are renamed into a per-volume holding folder and can be restored from History, `cleanprojects undo` or `cleanprojects recovery`; automatic purge after `clean.recovery_days`.
- Every mounted volume: external SSDs, USB drives and network shares are listed, external writable volumes are scanned for projects, their trash is offered, and recovery bins live on the same volume.
- Explorer: squarified treemap and list browser in the desktop app, a directory browser tab in the TUI, `cleanprojects explore` on the CLI.
- Menu bar / system tray with recoverable size, close-to-tray, scheduled background scans with notifications, launch at login, single instance, in-app updater (signed releases).
- Guards: artifacts with git-tracked files, nested repositories or private keys are never offered; items owned by a running application are blocked until it is closed (`--force` to override).
- Accurate sizes: allocated bytes with hardlinks counted once; apparent size kept for reference.
- Coverage: Godot, Unreal, Zig, Haskell, Terraform, Jupyter, Pixi projects; per-profile Cargo `target/`; Docker/Podman per unused image; iOS simulators per device; installers in Downloads; superseded iOS DeviceSupport and JetBrains versions; LM Studio, Playwright, Puppeteer, Cypress, Electron, Helm, Terraform caches and more.
- Candidates can be grouped by category, app, project or volume; owner shown in details and reports.
- Last scan is cached and shown instantly on launch while the rescan runs.
- `cleanprojects scan --json-stream`, `cleanprojects volumes`, `cleanprojects skill --install` (agent skill for Claude Code, Codex, Cursor).
- Packaging templates (Homebrew formula and cask, Scoop, winget, AUR) and `cargo binstall` metadata.

### Changed
- `clean.use_trash` is superseded by `clean.mode` (`recovery`, `trash`, `permanent`); old config files keep working.

## 2.0.0

Complete rewrite in Rust. The Go + React macOS app moved to `legacy/v1`.

### Added
- Cross-platform engine (`cleanprojects-core`): 120+ cache locations for macOS, Linux and Windows, ecosystem-aware project artifact detection (Node, Rust, Go, Python, PHP, Gradle, Maven, Swift, .NET, Flutter, Ruby, Elixir, CMake, Unity), stale project tagging.
- Git maintenance: prunable worktree records, idle clean worktrees (merged vs unmerged risk), merged local branches (opt-in), `git gc` suggestions for repositories with many loose objects.
- Tool probes: Docker and Podman reclaimable space, pnpm store prune, iOS simulator runtimes, conda.
- Desktop app (Tauri 2 + React): overview with disk ring and category chart, candidates table with detail panel, git view, history with trend, settings, light and dark themes, English and Turkish.
- Terminal UI (ratatui) with the same workflow, keyboard driven.
- Scriptable CLI: `scan --json`, `clean --dry-run`, filters by category, risk, stale, id; exclusions, history, config, doctor, shell completions.
- Trash mode (default) with permanent-delete fallback, dry run, safety re-validation before every deletion.
- GitHub Actions: CI on three operating systems, release workflow producing terminal binaries for five targets and desktop installers, optional macOS signing and notarization.
