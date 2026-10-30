# Packaging

Templates for package managers. Each release: bump `version`, fill the SHA-256
values from `SHA256SUMS.txt` on the GitHub release, and publish.

| File | Target | How users install |
|------|--------|-------------------|
| `homebrew/cleanprojects.rb` | Homebrew formula (terminal) | `brew install furkanmeclis/tap/cleanprojects` |
| `homebrew/cleanprojects-desktop.rb` | Homebrew cask (desktop) | `brew install --cask furkanmeclis/tap/cleanprojects-desktop` |
| `scoop/cleanprojects.json` | Scoop (Windows terminal) | `scoop install cleanprojects` from a bucket |
| `winget/FurkanMeclis.CleanProjects.yaml` | winget (Windows desktop) | `winget install FurkanMeclis.CleanProjects` |
| `aur/PKGBUILD` | Arch Linux | `yay -S cleanprojects-bin` |

`cargo binstall cleanprojects` works without any of these: the binary metadata
in `crates/cli/Cargo.toml` points at the release archives.
