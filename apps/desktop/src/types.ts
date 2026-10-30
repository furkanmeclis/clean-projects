export type Risk = "low" | "medium" | "high";
export type Category =
  | "project"
  | "package-cache"
  | "build-cache"
  | "ide"
  | "browser"
  | "container"
  | "mobile"
  | "git"
  | "app-data"
  | "temp";

export type Action =
  | { type: "remove-path"; path: string }
  | { type: "run-command"; program: string; args: string[]; cwd?: string | null }
  | { type: "git-worktree-prune"; repo: string }
  | { type: "git-worktree-remove"; repo: string; worktree: string }
  | { type: "git-branch-delete"; repo: string; branch: string }
  | { type: "git-gc"; repo: string };

export interface ProjectInfo {
  root: string;
  name: string;
  kind: string;
  idle_days: number;
}

export interface Candidate {
  id: string;
  label: string;
  description: string;
  category: Category;
  risk: Risk;
  size: number;
  size_exact: boolean;
  modified?: string | null;
  action: Action;
  project?: ProjectInfo | null;
  tags?: string[];
}

export interface DiskInfo {
  mount: string;
  total: number;
  free: number;
  used: number;
}

export interface Config {
  scan: {
    roots: string[];
    default_roots: boolean;
    max_depth: number;
    min_size_mb: number;
    stale_days: number;
    protected: string[];
    disabled_rules: string[];
  };
  git: {
    enabled: boolean;
    merged_branches: boolean;
    gc: boolean;
    worktree_idle_days: number;
  };
  clean: { use_trash: boolean };
  ui: { language: "en" | "tr" };
}

export interface HistoryItem {
  id: string;
  label: string;
  detail: string;
  size: number;
  ok: boolean;
  error?: string | null;
}

export interface HistoryEntry {
  at: string;
  freed: number;
  dry_run: boolean;
  trashed: boolean;
  items: HistoryItem[];
}

export interface Bootstrap {
  version: string;
  os: string;
  home: string;
  config_path: string;
  config: Config;
  excluded: string[];
  history: HistoryEntry[];
  disk: DiskInfo;
  roots: string[];
  protected: string[];
  tools: [string, boolean][];
  candidates: Candidate[];
  scanning: boolean;
}

export interface ScanSummary {
  candidates: number;
  total: number;
  projects: number;
  repos: number;
  duration_ms: number;
  disk: DiskInfo;
}

export interface CleanDone {
  entry: HistoryEntry;
  disk: DiskInfo;
  removed_ids: string[];
}

export const CATEGORIES: Category[] = [
  "project",
  "package-cache",
  "build-cache",
  "ide",
  "browser",
  "container",
  "mobile",
  "git",
  "app-data",
  "temp",
];

export function actionDisplay(a: Action): string {
  switch (a.type) {
    case "remove-path":
      return a.path;
    case "run-command":
      return [a.program, ...a.args].join(" ");
    case "git-worktree-prune":
      return `git -C ${a.repo} worktree prune`;
    case "git-worktree-remove":
      return `git -C ${a.repo} worktree remove ${a.worktree}`;
    case "git-branch-delete":
      return `git -C ${a.repo} branch -d ${a.branch}`;
    case "git-gc":
      return `git -C ${a.repo} gc --prune=now`;
  }
}

export function actionRepo(a: Action): string | null {
  switch (a.type) {
    case "git-worktree-prune":
    case "git-worktree-remove":
    case "git-branch-delete":
    case "git-gc":
      return a.repo;
    default:
      return null;
  }
}

export function actionPath(a: Action): string | null {
  switch (a.type) {
    case "remove-path":
      return a.path;
    case "git-worktree-remove":
      return a.worktree;
    case "git-worktree-prune":
    case "git-branch-delete":
    case "git-gc":
      return a.repo;
    default:
      return a.cwd ?? null;
  }
}
