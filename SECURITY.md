# Security Policy

CleanProjects is a local disk cleanup utility. It never uploads data, never
runs as root, and only removes what you select after an explicit confirmation.

Please report security issues privately through
[GitHub Security Advisories](https://github.com/furkanmeclis/clean-projects/security/advisories/new).

Do not include private file contents, secrets, or full filesystem listings in
public issues.

## Supported versions

| Version | Supported |
|---------|-----------|
| 2.x     | yes       |
| 1.x     | no (legacy, `legacy/v1`) |

## Safety model

- Cloud-synced and personal folders (iCloud Drive, CloudStorage, Dropbox,
  Google Drive, OneDrive, `.ssh`, `.gnupg`, `.aws`, …) are never scanned or
  deleted. You can add more in `config.toml`.
- Every deletion re-validates the path immediately before removal: absolute,
  not a filesystem root, not the home directory, not inside a protected
  location, not inside a `.git` directory, not a symlink.
- Trash mode is the default; permanent deletion must be chosen explicitly.
- `git branch -d` (never `-D`) is used for merged branches, and worktrees are
  only offered when their working tree is clean.
