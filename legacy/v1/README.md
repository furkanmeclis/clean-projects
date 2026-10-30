# CleanProjects v1 (legacy)

This folder keeps the original macOS-only implementation: a Go backend that
serves an embedded React dashboard on `http://127.0.0.1:51392`.

It is no longer developed. Version 2 (the Rust workspace at the repository root)
replaces it with a cross-platform core, a desktop app, a terminal UI and a CLI.

To build v1 anyway:

```bash
cd legacy/v1
make build
./clean-projects
```

The former release workflow is kept as `release.workflow.yml` for reference.
