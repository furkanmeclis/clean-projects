# Homebrew cask for the desktop app (same tap):
#   brew install --cask furkanmeclis/tap/cleanprojects-desktop
cask "cleanprojects-desktop" do
  version "0.3.1"
  sha256 arm:   "REPLACE_WITH_SHA256",
         intel: "REPLACE_WITH_SHA256"

  url "https://github.com/furkanmeclis/clean-projects/releases/download/v#{version}/CleanProjects_#{version}_#{Hardware::CPU.arm? ? "aarch64" : "x64"}.dmg"
  name "CleanProjects"
  desc "Developer disk cleanup with a recovery bin, git maintenance and a treemap explorer"
  homepage "https://github.com/furkanmeclis/clean-projects"

  app "CleanProjects.app"

  zap trash: [
    "~/Library/Application Support/cleanprojects",
    "~/Library/Application Support/com.furkanmeclis.cleanprojects",
  ]
end
