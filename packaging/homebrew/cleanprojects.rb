# Homebrew formula for the terminal binary. Publish it in a tap repository
# (e.g. furkanmeclis/homebrew-tap) and update `version` + `sha256` per release:
#   brew tap furkanmeclis/tap && brew install cleanprojects
class Cleanprojects < Formula
  desc "Developer disk cleanup: caches, build artifacts, Docker and git worktrees (TUI + CLI)"
  homepage "https://github.com/furkanmeclis/clean-projects"
  version "2.3.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/furkanmeclis/clean-projects/releases/download/v#{version}/cleanprojects-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    end
    on_intel do
      url "https://github.com/furkanmeclis/clean-projects/releases/download/v#{version}/cleanprojects-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/furkanmeclis/clean-projects/releases/download/v#{version}/cleanprojects-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    end
    on_intel do
      url "https://github.com/furkanmeclis/clean-projects/releases/download/v#{version}/cleanprojects-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    end
  end

  def install
    bin.install Dir["cleanprojects-*/cleanprojects"].first
    generate_completions_from_executable(bin/"cleanprojects", "completions")
  end

  test do
    assert_match "CleanProjects doctor", shell_output("#{bin}/cleanprojects doctor")
  end
end
