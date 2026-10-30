# Changelog

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
