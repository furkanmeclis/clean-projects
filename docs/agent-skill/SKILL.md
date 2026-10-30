---
name: cleanprojects
description: Reclaim developer disk space safely with the cleanprojects CLI. Use when the user asks to free disk space, clean caches, node_modules, build artifacts, Docker, or stale git worktrees.
---

# CleanProjects

`cleanprojects` scans package caches, build output, IDE/browser caches, container engines and git repositories, then removes only what is explicitly selected. Nothing is deleted without `--yes`, and the default disposal mode is an undoable recovery bin.

## Workflow

1. Inspect first, never delete blindly:
   ```bash
   cleanprojects scan --json            # all candidates with id, size, risk, category, owner, blocked_by
   cleanprojects scan --json-stream     # one JSON object per line while scanning (large machines)
   cleanprojects volumes                # mounted disks and whether they are scanned
   cleanprojects explore ~/Library      # largest entries of a directory
   ```
2. Show the user a short summary (total recoverable, top items, anything with `"risk": "high"` or a `blocked_by` app).
3. Clean only what the user approved. Prefer filters over ids when the intent is broad:
   ```bash
   cleanprojects clean --dry-run --max-risk low            # preview
   cleanprojects clean --yes --max-risk low                # safe sweep, recovery bin
   cleanprojects clean --yes --category git                # worktree prune / gc
   cleanprojects clean --yes --stale                       # artifacts of idle projects
   cleanprojects clean --yes --id <ID> --id <ID>           # specific candidates
   ```
4. Report what was freed. Mention that `cleanprojects undo` restores the last run and `cleanprojects recovery --purge` releases the bin.

## Rules

- Never pass `--permanent` or `--force` unless the user explicitly asked for permanent deletion or to override a running-app block.
- Never clean `--max-risk high` items without naming them to the user first; they may reset application state.
- Items whose `blocked_by` is set require that application to be closed; tell the user instead of forcing.
- Protected locations (iCloud, CloudStorage, Dropbox, `.ssh`, …) are never scanned; do not try to work around that.
- `clean` refuses to run without `--yes` when stdin is not a terminal; that is intentional.

## Useful flags

`--root DIR` add a project root · `--only-roots` scan just those · `--no-git` `--no-tools` `--no-global` `--no-extras` `--no-volumes` narrow the scan · `--category project|package-cache|build-cache|ide|browser|container|mobile|git|app-data|temp` · `--query TEXT` · `--mode recovery|trash|permanent` · `--lang tr` for Turkish output.
