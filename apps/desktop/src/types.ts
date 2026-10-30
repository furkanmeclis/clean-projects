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
  apparent_size?: number;
  modified?: string | null;
  owner?: string;
  blocked_by?: string | null;
  volume?: string | null;
  action: Action;
  project?: ProjectInfo | null;
  tags?: string[];
}

export type CleanMode = "recovery" | "trash" | "permanent";

export interface Volume {
  name: string;
  mount: string;
  total: number;
  free: number;
  used: number;
  kind: "system" | "external" | "network";
  removable: boolean;
  read_only: boolean;
  file_system: string;
}

export interface RecoveryEntry {
  id: string;
  run_at: string;
  candidate_id: string;
  label: string;
  original: string;
  stored: string;
  size: number;
}

export interface RecoverySummary {
  items: number;
  bytes: number;
  entries: RecoveryEntry[];
}

export interface Node {
  name: string;
  path: string;
  size: number;
  files: number;
  is_dir: boolean;
  is_symlink: boolean;
  modified?: string | null;
}

export interface TreeNode extends Node {
  children: TreeNode[];
  other: number;
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
    volumes: boolean;
    skip_volumes: string[];
    installer_days: number;
  };
  git: {
    enabled: boolean;
    merged_branches: boolean;
    gc: boolean;
    worktree_idle_days: number;
  };
  clean: { use_trash: boolean; mode: CleanMode; recovery_days: number; block_running_apps: boolean };
  ui: { language: "en" | "tr"; close_to_tray: boolean; scan_interval_hours: number; notify_threshold_gb: number; check_updates: boolean };
}

export interface HistoryItem {
  id: string;
  label: string;
  detail: string;
  size: number;
  ok: boolean;
  error?: string | null;
  recovery_id?: string | null;
}

export interface HistoryEntry {
  at: string;
  freed: number;
  dry_run: boolean;
  trashed: boolean;
  mode?: string;
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
  data_dir: string;
  disk: DiskInfo;
  volumes: Volume[];
  roots: string[];
  protected: string[];
  tools: [string, boolean][];
  candidates: Candidate[];
  cached: boolean;
  guarded: number;
  scanning: boolean;
  recovery: RecoverySummary;
}

export interface ScanSummary {
  candidates: number;
  total: number;
  projects: number;
  repos: number;
  guarded: number;
  duration_ms: number;
  disk: DiskInfo;
  volumes: Volume[];
  background: boolean;
}

export interface CleanDone {
  entry: HistoryEntry;
  disk: DiskInfo;
  removed_ids: string[];
  recovery: RecoverySummary;
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
