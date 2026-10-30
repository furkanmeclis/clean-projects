// Bridge to the Tauri backend. When the page runs outside Tauri (plain browser,
// screenshots, UI development) a mock implementation with sample data is used.

import type { Bootstrap, Candidate, CleanDone, CleanMode, Config, HistoryEntry, Node, RecoverySummary, ScanSummary, TreeNode, Volume } from "./types";

export interface CleanRequest {
  ids: string[];
  dry_run: boolean;
  mode?: CleanMode | null;
  force?: boolean;
}

export interface UpdateInfo {
  version: string;
  body?: string | null;
  install: () => Promise<void>;
}

export interface Api {
  bootstrap(): Promise<Bootstrap>;
  startScan(options?: { no_git?: boolean; no_tools?: boolean; background?: boolean }): Promise<void>;
  startClean(request: CleanRequest): Promise<void>;
  undoRun(index: number): Promise<[number, string[]]>;
  restoreEntry(id: string): Promise<RecoverySummary>;
  recoveryPurge(all: boolean): Promise<RecoverySummary>;
  toggleExclude(id: string): Promise<boolean>;
  saveConfig(config: Config): Promise<Config>;
  getHistory(): Promise<HistoryEntry[]>;
  clearHistory(): Promise<void>;
  listVolumes(): Promise<Volume[]>;
  exploreDir(path: string): Promise<Node[]>;
  exploreTree(path: string, depth?: number, top?: number): Promise<TreeNode[]>;
  addManualCandidate(path: string): Promise<Candidate>;
  writeReport(path: string, ids: string[]): Promise<string>;
  reveal(path: string): Promise<void>;
  saveDialog(defaultName: string): Promise<string | null>;
  checkUpdate(): Promise<UpdateInfo | null>;
  relaunch(): Promise<void>;
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
    undoRun: (index) => invoke<[number, string[]]>("undo_run", { index }),
    restoreEntry: (id) => invoke<RecoverySummary>("restore_entry", { id }),
    recoveryPurge: (all) => invoke<RecoverySummary>("recovery_purge", { all }),
    toggleExclude: (id) => invoke<boolean>("toggle_exclude", { id }),
    saveConfig: (config) => invoke<Config>("save_config", { config }),
    getHistory: () => invoke<HistoryEntry[]>("get_history"),
    clearHistory: () => invoke("clear_history"),
    listVolumes: () => invoke<Volume[]>("list_volumes"),
    exploreDir: (path) => invoke<Node[]>("explore_dir", { path }),
    exploreTree: (path, depth, top) => invoke<TreeNode[]>("explore_tree", { path, depth, top }),
    addManualCandidate: (path) => invoke<Candidate>("add_manual_candidate", { path }),
    writeReport: (path, ids) => invoke<string>("write_report", { path, ids }),
    checkUpdate: async () => {
      try {
        const { check } = await import("@tauri-apps/plugin-updater");
        const u = await check();
        if (!u) return null;
        return { version: u.version, body: u.body, install: () => u.downloadAndInstall() };
      } catch {
        return null;
      }
    },
    relaunch: async () => {
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    },
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
    { ...c("a5", "Cursor global state DB", "app-data", "high", 1.9e9, `${home}/Library/Application Support/Cursor/User/globalStorage/state.vscdb`, "Close Cursor first; settings and chat state may reset.", 0), blocked_by: "Cursor", owner: "Cursor" },
    { ...c("a12", "Docker image postgres:15", "container", "medium", 412e6, "docker rmi 9f1c2a", "Image created 2 months ago, not used by any container.", null, false), owner: "Docker", tags: ["image"] },
    { ...c("a13", "Installer Xcode_16.1.xip", "temp", "low", 3.2e9, `${home}/Downloads/Xcode_16.1.xip`, "XIP file in Downloads, 41 days old.", 41), owner: "Downloads", tags: ["installer"] },
    { ...c("a14", "mobile-app · build", "project", "low", 920e6, `/Volumes/Work SSD/mobile-app/build`, "Build output. Project: mobile-app (flutter), idle 12 days.", 12, true, { root: "/Volumes/Work SSD/mobile-app", name: "mobile-app", kind: "flutter", idle_days: 12 }, ["flutter"]), owner: "mobile-app", volume: "/Volumes/Work SSD" },
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
    scan: { roots: ["~/Projects", "~/Documents"], default_roots: true, max_depth: 6, min_size_mb: 1, stale_days: 30, protected: [], disabled_rules: [], volumes: true, skip_volumes: [], installer_days: 7 },
    git: { enabled: true, merged_branches: false, gc: true, worktree_idle_days: 7 },
    clean: { use_trash: true, mode: "recovery", recovery_days: 14, block_running_apps: true },
    ui: { language: "en", close_to_tray: true, scan_interval_hours: 24, notify_threshold_gb: 5, check_updates: true },
  };
  const disk = { mount: "/", total: 994e9, free: 182e9, used: 812e9 };
  const vols: Volume[] = [
    { name: "Macintosh HD", mount: "/", total: 994e9, free: 182e9, used: 812e9, kind: "system", removable: false, read_only: false, file_system: "apfs" },
    { name: "Work SSD", mount: "/Volumes/Work SSD", total: 2000e9, free: 640e9, used: 1360e9, kind: "external", removable: true, read_only: false, file_system: "apfs" },
    { name: "Archive", mount: "/Volumes/Archive", total: 4000e9, free: 2900e9, used: 1100e9, kind: "external", removable: true, read_only: false, file_system: "exfat" },
  ];
  let recovery: RecoverySummary = { items: 2, bytes: 1.6e9 + 310e6, entries: [
    { id: "20261027-1541-node_modules", run_at: new Date(Date.now() - 86400000 * 3).toISOString(), candidate_id: "y", label: "old-app · node_modules", original: `${home}/Projects/old-app/node_modules`, stored: `${home}/Library/Application Support/cleanprojects/recovery/20261027-1541/node_modules`, size: 1.6e9 },
    { id: "20261027-1541-vendor", run_at: new Date(Date.now() - 86400000 * 3).toISOString(), candidate_id: "z", label: "shop · vendor", original: `${home}/Projects/shop/vendor`, stored: `${home}/Library/Application Support/cleanprojects/recovery/20261027-1541/vendor`, size: 310e6 },
  ] };
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
        data_dir: `${home}/Library/Application Support/cleanprojects`,
        volumes: vols,
        candidates,
        cached: false,
        guarded: 3,
        scanning: false,
        recovery,
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
          emit("scan://done", { candidates: candidates.length, total: candidates.reduce((s, x) => s + x.size, 0), projects: 14, repos: 9, guarded: 3, duration_ms: 2140, disk, volumes: vols, background: false } satisfies ScanSummary);
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
          const mode = req.mode ?? "recovery";
          const entry: HistoryEntry = { at: new Date().toISOString(), freed: done.reduce((s, d) => s + d.size, 0), dry_run: req.dry_run, trashed: mode !== "permanent", mode, items: done.map((d) => ({ ...d, recovery_id: mode === "recovery" && d.ok && !req.dry_run ? `run-${d.id}` : null })) };
          history = [...history, entry];
          if (mode === "recovery" && !req.dry_run) {
            const more = done.filter((d) => d.ok).map((d) => ({ id: `run-${d.id}`, run_at: entry.at, candidate_id: d.id, label: d.label, original: d.detail || `${home}/Projects/${d.label}`, stored: `${home}/Library/Application Support/cleanprojects/recovery/run/${d.label}`, size: d.size }));
            recovery = { items: recovery.items + more.length, bytes: recovery.bytes + entry.freed, entries: [...recovery.entries, ...more] };
          }
          emit("clean://done", { entry, disk: { ...disk, free: disk.free + entry.freed }, removed_ids: removed, recovery } satisfies CleanDone);
          return;
        }
        const it = items[i];
        emit("clean://start", { index: i, total: items.length, id: it.id, label: it.label });
        emit("clean://log", `  ${req.dry_run ? "[dry-run] would remove" : "trash"} ${it.action.type === "remove-path" ? it.action.path : "…"}`);
        const ok = it.id !== "a5";
        setTimeout(() => {
          emit("clean://item", { id: it.id, ok, freed: ok ? it.size : 0, freed_human: "", error: ok ? null : "Cursor is running; close it first" });
          done.push({ id: it.id, label: it.label, detail: it.action.type === "remove-path" ? it.action.path : "", size: ok ? it.size : 0, ok, error: ok ? null : "Cursor is running; close it first" });
          i++;
          step();
        }, 350);
      };
      setTimeout(step, 300);
    },
    async undoRun(index) {
      const h = history[index];
      if (!h) return [0, ["no such run"]];
      const n = h.items.filter((i) => i.recovery_id).length;
      h.items.forEach((i) => (i.recovery_id = null));
      recovery = { ...recovery, items: Math.max(0, recovery.items - n), entries: recovery.entries.filter((e) => !h.items.some((i) => e.candidate_id === i.id)) };
      return [n, []];
    },
    async restoreEntry(id) {
      recovery = { ...recovery, items: recovery.items - 1, entries: recovery.entries.filter((e) => e.id !== id) };
      recovery.bytes = recovery.entries.reduce((s, e) => s + e.size, 0);
      return recovery;
    },
    async recoveryPurge(all) {
      recovery = all ? { items: 0, bytes: 0, entries: [] } : recovery;
      return recovery;
    },
    async listVolumes() {
      return vols;
    },
    async exploreDir(path) {
      const base = path.replace(/\/$/, "");
      const mk = (name: string, size: number, is_dir = true): Node => ({ name, path: `${base}/${name}`, size, files: is_dir ? Math.round(size / 50000) : 1, is_dir, is_symlink: false, modified: new Date().toISOString() });
      if (base.endsWith("/Library")) return [mk("Developer", 41e9), mk("Caches", 12e9), mk("Application Support", 9.4e9), mk("Containers", 3.1e9), mk("Logs", 700e6)];
      if (base.endsWith("/Projects")) return [mk("api-core", 7.2e9), mk("shop-frontend", 3.9e9), mk("mobile-app", 2.2e9), mk("infra", 400e6), mk("README.md", 4e3, false)];
      return [mk("Library", 66e9), mk("Projects", 13.7e9), mk("Movies", 22e9), mk("Downloads", 8.2e9), mk("Documents", 4.1e9), mk("Pictures", 2.4e9), mk(".cache", 1.9e9), mk("Desktop", 300e6)];
    },
    async exploreTree(path) {
      const top = await this.exploreDir(path);
      const out: TreeNode[] = [];
      for (const n of top) {
        const kids = n.is_dir ? await this.exploreDir(n.path) : [];
        const scale = n.size / Math.max(1, kids.reduce((s, k) => s + k.size, 0));
        out.push({ ...n, other: 0, children: kids.map((k) => ({ ...k, size: k.size * scale, children: [], other: 0 })) });
      }
      return out;
    },
    async addManualCandidate(path) {
      const name = path.split("/").pop() ?? path;
      const c: Candidate = { id: `m-${name}`, label: name, description: "Picked in the explorer.", category: "temp", risk: "high", size: 1.2e9, size_exact: true, modified: null, action: { type: "remove-path", path }, project: null, tags: ["manual"], owner: "Explorer" };
      candidates.push(c);
      return c;
    },
    async checkUpdate() {
      return null;
    },
    async relaunch() {},
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
      description, project: project ?? null, tags, owner: project?.name ?? label.split(" ")[0], volume: "/",
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
