// Bridge to the Tauri backend. When the page runs outside Tauri (plain browser,
// screenshots, UI development) a mock implementation with sample data is used.

import type { Bootstrap, Candidate, CleanDone, Config, HistoryEntry, ScanSummary } from "./types";

export interface CleanRequest {
  ids: string[];
  dry_run: boolean;
  use_trash?: boolean | null;
}

export interface Api {
  bootstrap(): Promise<Bootstrap>;
  startScan(options?: { no_git?: boolean; no_tools?: boolean }): Promise<void>;
  startClean(request: CleanRequest): Promise<void>;
  toggleExclude(id: string): Promise<boolean>;
  saveConfig(config: Config): Promise<Config>;
  getHistory(): Promise<HistoryEntry[]>;
  clearHistory(): Promise<void>;
  writeReport(path: string, ids: string[]): Promise<string>;
  reveal(path: string): Promise<void>;
  saveDialog(defaultName: string): Promise<string | null>;
  on<T>(event: EventName, handler: (payload: T) => void): Promise<() => void>;
}

export type EventName =
  | "scan://progress"
  | "scan://found"
  | "scan://done"
  | "clean://start"
  | "clean://log"
  | "clean://item"
  | "clean://done";

export interface ScanProgress {
  phase: string;
  message: string;
}
export interface CleanStart {
  index: number;
  total: number;
  id: string;
  label: string;
}
export interface CleanItem {
  id: string;
  ok: boolean;
  freed: number;
  freed_human: string;
  error?: string | null;
}

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function tauriApi(): Promise<Api> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  const opener = await import("@tauri-apps/plugin-opener");
  const dialog = await import("@tauri-apps/plugin-dialog");
  return {
    bootstrap: () => invoke<Bootstrap>("bootstrap"),
    startScan: (options) => invoke("start_scan", { options: options ?? {} }),
    startClean: (request) => invoke("start_clean", { request }),
    toggleExclude: (id) => invoke<boolean>("toggle_exclude", { id }),
    saveConfig: (config) => invoke<Config>("save_config", { config }),
    getHistory: () => invoke<HistoryEntry[]>("get_history"),
    clearHistory: () => invoke("clear_history"),
    writeReport: (path, ids) => invoke<string>("write_report", { path, ids }),
    reveal: (path) => opener.revealItemInDir(path),
    saveDialog: (defaultName) =>
      dialog.save({ defaultPath: defaultName, filters: [{ name: "Markdown", extensions: ["md"] }] }),
    on: async (event, handler) => {
      const un = await listen(event, (e) => handler(e.payload as never));
      return un;
    },
  };
}

// ───────────────────────── mock for browser development ─────────────────────────

type Handler = (payload: unknown) => void;

function mockApi(): Api {
  const listeners = new Map<EventName, Set<Handler>>();
  const emit = (event: EventName, payload: unknown) => listeners.get(event)?.forEach((h) => h(payload));
  const home = "/Users/dev";
  const sample: Candidate[] = [
    c("a1", "Xcode DerivedData", "build-cache", "low", 18.4e9, `${home}/Library/Developer/Xcode/DerivedData`, "Xcode build output and indexes. Rebuilt on next build.", 3),
    c("a2", "Docker build cache", "container", "low", 9.7e9, "docker builder prune -af", "Prunes the BuildKit layer cache only.", null, false),
    c("a3", "shop-frontend · node_modules", "project", "low", 2.3e9, `${home}/Projects/shop-frontend/node_modules`, "Installed npm dependencies. Project: shop-frontend (node), idle 84 days.", 84, true, { root: `${home}/Projects/shop-frontend`, name: "shop-frontend", kind: "node", idle_days: 84 }, ["node", "stale"]),
    c("a4", "Gradle caches", "package-cache", "medium", 4.1e9, `${home}/.gradle/caches`, "Gradle dependency and build caches.", 12),
    c("a5", "Cursor global state DB", "app-data", "high", 1.9e9, `${home}/Library/Application Support/Cursor/User/globalStorage/state.vscdb`, "Close Cursor first; settings and chat state may reset.", 0),
    c("a6", "api-core · target", "project", "low", 6.2e9, `${home}/Projects/api-core/target`, "Compiler output (cargo). Project: api-core (rust), idle 2 days.", 2, true, { root: `${home}/Projects/api-core`, name: "api-core", kind: "rust", idle_days: 2 }, ["rust"]),
    c("a7", "Chrome cache", "browser", "low", 1.2e9, `${home}/Library/Caches/Google/Chrome`, "Google Chrome cache.", 0),
    c("a8", "Simulator runtime iOS 17.5", "mobile", "medium", 7.6e9, "xcrun simctl runtime delete 1234-ABCD", "Deletes the simulator runtime disk image.", null, false),
    gitc("g1", "api-core · worktree hotfix-2024", "git-worktree-remove", `${home}/Projects/api-core`, 410e6, "Clean worktree on `hotfix-2024`, idle 61 days. Branch is merged into the default branch.", "medium", ["worktree", "merged"], `${home}/Projects/api-core-hotfix`),
    gitc("g2", "shop-frontend · prune 2 worktree record(s)", "git-worktree-prune", `${home}/Projects/shop-frontend`, 0, "Worktree directories that no longer exist: feat-a, feat-b.", "low", ["worktree"]),
    gitc("g3", "api-core · git gc", "git-gc", `${home}/Projects/api-core`, 220e6, "4210 loose objects (430 MB). git gc repacks them.", "low", ["gc"]),
    c("a9", "pnpm store", "package-cache", "medium", 3.4e9, `${home}/Library/pnpm/store`, "pnpm content-addressable store.", 5),
    c("a10", "JetBrains caches", "ide", "low", 2.8e9, `${home}/Library/Caches/JetBrains`, "IDE index caches. Projects re-index.", 1),
    c("a11", "legacy-site · vendor", "project", "low", 310e6, `${home}/Documents/legacy-site/vendor`, "Composer dependencies. Project: legacy-site (php), idle 240 days.", 240, true, { root: `${home}/Documents/legacy-site`, name: "legacy-site", kind: "php", idle_days: 240 }, ["php", "stale"]),
  ];
  let candidates: Candidate[] = [];
  let excluded = new Set<string>(["a7"]);
  let history: HistoryEntry[] = [
    {
      at: new Date(Date.now() - 86400000 * 3).toISOString(),
      freed: 12.6e9,
      dry_run: false,
      trashed: true,
      items: [
        { id: "x", label: "Xcode DerivedData", detail: `${home}/Library/Developer/Xcode/DerivedData`, size: 11e9, ok: true },
        { id: "y", label: "old-app · node_modules", detail: `${home}/Projects/old-app/node_modules`, size: 1.6e9, ok: true },
      ],
    },
  ];
  let config: Config = {
    scan: { roots: ["~/Projects", "~/Documents"], default_roots: true, max_depth: 6, min_size_mb: 1, stale_days: 30, protected: [], disabled_rules: [] },
    git: { enabled: true, merged_branches: false, gc: true, worktree_idle_days: 7 },
    clean: { use_trash: true },
    ui: { language: "en" },
  };
  const disk = { mount: "/", total: 994e9, free: 182e9, used: 812e9 };
  return {
    async bootstrap() {
      return {
        version: "2.0.0-mock",
        os: "macos",
        home,
        config_path: `${home}/.config/cleanprojects/config.toml`,
        config,
        excluded: [...excluded],
        history,
        disk,
        roots: ["~/Projects", "~/Documents", "~/dev"],
        protected: ["~/Library/Mobile Documents", "~/Library/CloudStorage", "~/Dropbox", "~/.ssh"],
        tools: [["git", true], ["docker", true], ["podman", false], ["pnpm", true], ["xcrun", true], ["conda", false], ["brew", true]],
        candidates,
        scanning: false,
      };
    },
    async startScan() {
      candidates = [];
      const phases = ["disk", "global", "tools", "projects", "git"];
      let i = 0;
      const tick = () => {
        if (i < phases.length) emit("scan://progress", { phase: phases[i], message: `~/Projects/${["shop-frontend", "api-core", "legacy-site", "mobile-app", "infra"][i]}` });
        if (i < sample.length) {
          candidates.push(sample[i]);
          emit("scan://found", sample[i]);
        }
        i++;
        if (i <= sample.length) setTimeout(tick, 140);
        else {
          emit("scan://done", { candidates: candidates.length, total: candidates.reduce((s, x) => s + x.size, 0), projects: 14, repos: 9, duration_ms: 2140, disk } satisfies ScanSummary);
        }
      };
      setTimeout(tick, 200);
    },
    async startClean(req) {
      const items = candidates.filter((x) => req.ids.includes(x.id));
      let i = 0;
      const done: HistoryEntry["items"] = [];
      const step = () => {
        if (i >= items.length) {
          const removed = req.dry_run ? [] : done.filter((d) => d.ok).map((d) => d.id);
          candidates = candidates.filter((x) => !removed.includes(x.id));
          const entry: HistoryEntry = { at: new Date().toISOString(), freed: done.reduce((s, d) => s + d.size, 0), dry_run: req.dry_run, trashed: req.use_trash ?? true, items: done };
          history = [...history, entry];
          emit("clean://done", { entry, disk: { ...disk, free: disk.free + entry.freed }, removed_ids: removed } satisfies CleanDone);
          return;
        }
        const it = items[i];
        emit("clean://start", { index: i, total: items.length, id: it.id, label: it.label });
        emit("clean://log", `  ${req.dry_run ? "[dry-run] would remove" : "trash"} ${it.action.type === "remove-path" ? it.action.path : "…"}`);
        const ok = it.id !== "a5";
        setTimeout(() => {
          emit("clean://item", { id: it.id, ok, freed: ok ? it.size : 0, freed_human: "", error: ok ? null : "Resource busy: close Cursor first" });
          done.push({ id: it.id, label: it.label, detail: "", size: ok ? it.size : 0, ok, error: ok ? null : "Resource busy" });
          i++;
          step();
        }, 350);
      };
      setTimeout(step, 300);
    },
    async toggleExclude(id) {
      if (excluded.has(id)) {
        excluded.delete(id);
        return false;
      }
      excluded.add(id);
      return true;
    },
    async saveConfig(c) {
      config = c;
      return c;
    },
    async getHistory() {
      return history;
    },
    async clearHistory() {
      history = [];
    },
    async writeReport(path) {
      return path;
    },
    async reveal() {},
    async saveDialog(name) {
      return `${home}/Desktop/${name}`;
    },
    async on(event, handler) {
      if (!listeners.has(event)) listeners.set(event, new Set());
      listeners.get(event)!.add(handler as Handler);
      return () => listeners.get(event)?.delete(handler as Handler);
    },
  };

  function c(id: string, label: string, category: Candidate["category"], risk: Candidate["risk"], size: number, pathOrCmd: string, description: string, ageDays: number | null, exact = true, project?: Candidate["project"], tags: string[] = []): Candidate {
    const isCmd = !pathOrCmd.startsWith("/");
    const [program, ...args] = pathOrCmd.split(" ");
    return {
      id, label, category, risk, size: Math.round(size), size_exact: exact,
      modified: ageDays === null ? null : new Date(Date.now() - ageDays * 86400000).toISOString(),
      action: isCmd ? { type: "run-command", program, args } : { type: "remove-path", path: pathOrCmd },
      description, project: project ?? null, tags,
    };
  }
  function gitc(id: string, label: string, type: "git-worktree-remove" | "git-worktree-prune" | "git-gc", repo: string, size: number, description: string, risk: Candidate["risk"], tags: string[], worktree?: string): Candidate {
    const action = type === "git-worktree-remove" ? { type, repo, worktree: worktree! } : type === "git-gc" ? { type, repo } : { type, repo };
    return { id, label, category: "git", risk, size: Math.round(size), size_exact: type !== "git-gc", modified: null, action: action as Candidate["action"], description, project: null, tags };
  }
}

let apiPromise: Promise<Api> | null = null;
export function getApi(): Promise<Api> {
  if (!apiPromise) apiPromise = isTauri ? tauriApi() : Promise.resolve(mockApi());
  return apiPromise;
}
