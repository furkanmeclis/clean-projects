import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowUpCircle, CalendarClock, ChevronDown, CircleCheckBig, Command, FileDown, FileJson, FileSpreadsheet, FileText, Flame, GitBranch, Github, History as HistoryIcon, Keyboard, LayoutDashboard, LayoutGrid, ListChecks, Loader2, Moon, MousePointerClick, RefreshCw, Settings as SettingsIcon, ShieldCheck, Sparkles, Sun, SunMoon, Trash2, X, Zap } from "lucide-react";
import { Routines } from "./views/Routines";
import { CommandPalette, type PaletteAction } from "./components/CommandPalette";
import { getApi, type Api, type CleanItem, type CleanStart, type ScanProgress, type UpdateInfo } from "./api";
import type { Bootstrap, Candidate, CleanDone, CleanMode, Config, DiskInfo, HistoryEntry, IntegrationStatus, LaunchAction, RecoverySummary, Routine, RoutineRun, ScanSnapshot, ScanSummary, Volume } from "./types";
import { Explore } from "./views/Explore";
import { humanSize } from "./format";
import { STRINGS, type Lang } from "./i18n";
import { Overview } from "./views/Overview";
import { Candidates } from "./views/Candidates";
import { GitView } from "./views/GitView";
import { History } from "./views/History";
import { Settings } from "./views/Settings";
import { CleanModal, type LogLine, type Phase } from "./components/CleanModal";

type Tab = "overview" | "candidates" | "git" | "explore" | "routines" | "history" | "settings";

export default function App() {
  const apiRef = useRef<Api | null>(null);
  const [boot, setBoot] = useState<Bootstrap | null>(null);
  const [tab, setTab] = useState<Tab>("overview");
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [excluded, setExcluded] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [activeId, setActiveId] = useState<string | null>(null);
  const [disk, setDisk] = useState<DiskInfo>({ mount: "", total: 0, free: 0, used: 0 });
  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [lastScan, setLastScan] = useState<ScanSummary | null>(null);
  const [hasScanned, setHasScanned] = useState(false);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [config, setConfig] = useState<Config | null>(null);
  const [modal, setModal] = useState<Phase | null>(null);
  const [mode, setMode] = useState<CleanMode>("recovery");
  const [force, setForce] = useState(false);
  const [recovery, setRecovery] = useState<RecoverySummary>({ items: 0, bytes: 0, entries: [] });
  const [volumes, setVolumes] = useState<Volume[]>([]);
  const [guarded, setGuarded] = useState(0);
  const [cached, setCached] = useState(false);
  const [exploreStart, setExploreStart] = useState<string | null>(null);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [scans, setScans] = useState<ScanSnapshot[]>([]);
  const [palette, setPalette] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [routines, setRoutines] = useState<Routine[]>([]);
  const [routineRuns, setRoutineRuns] = useState<RoutineRun[]>([]);
  const [routineRunning, setRoutineRunning] = useState(false);
  const [integration, setIntegration] = useState<IntegrationStatus>({ installed: false, platform: "", locations: [], note: "" });
  const [launch, setLaunch] = useState<LaunchAction | null>(null);
  const launchRef = useRef<LaunchAction | null>(null);
  const [dryRun, setDryRun] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [showKeys, setShowKeys] = useState(false);
  const logRef = useRef<LogLine[]>([]);
  const cleanRef = useRef<{ index: number; total: number; label: string }>({ index: 0, total: 0, label: "" });

  const lang: Lang = config?.ui.language ?? "en";
  const s = STRINGS[lang];
  const theme = config?.ui.theme ?? "system";
  useEffect(() => {
    const root = document.documentElement;
    if (theme === "system") delete root.dataset.theme;
    else root.dataset.theme = theme;
  }, [theme]);

  const showToast = useCallback((msg: string) => {
    setToast(msg);
    window.setTimeout(() => setToast(null), 2400);
  }, []);

  useEffect(() => {
    let disposed = false;
    const unsubs: Array<() => void> = [];
    (async () => {
      const api = await getApi();
      apiRef.current = api;
      const b = await api.bootstrap();
      if (disposed) return;
      setBoot(b);
      setConfig(b.config);
      setMode(b.config.clean.mode ?? "recovery");
      setExcluded(new Set(b.excluded));
      setHistory(b.history);
      setDisk(b.disk);
      setVolumes(b.volumes);
      setRecovery(b.recovery);
      setGuarded(b.guarded);
      setCached(b.cached);
      setScans(b.scans);
      setRoutines(b.routines);
      setRoutineRuns(b.routine_runs);
      setIntegration(b.integration);
      setCandidates(b.candidates);
      setHasScanned(b.candidates.length > 0);
      setScanning(b.scanning);
      if (b.config.ui.check_updates) api.checkUpdate().then((u) => { if (!disposed && u) setUpdate(u); }).catch(() => {});
      unsubs.push(await api.on<ScanProgress>("scan://progress", (p) => setProgress(p)));
      unsubs.push(await api.on<Candidate>("scan://found", (c) => {
        setCached((was) => { if (was) setCandidates([]); return false; });
        setCandidates((cs) => (cs.some((x) => x.id === c.id) ? cs : [...cs, c]));
      }));
      unsubs.push(await api.on<ScanSummary>("scan://done", (sum) => {
        setScanning(false);
        setProgress(null);
        setLastScan(sum);
        setDisk(sum.disk);
        setVolumes(sum.volumes);
        setGuarded(sum.guarded);
        setCached(false);
        setHasScanned(true);
        api.bootstrap().then((nb) => setScans(nb.scans)).catch(() => {});
        const la = launchRef.current;
        if (la) {
          launchRef.current = null;
          setLaunch(null);
          setTab("candidates");
          if (la.kind === "clean") {
            api.getCandidates().then((cs) => {
              const ids = cs.filter((c) => c.risk === "low" && !c.blocked_by).map((c) => c.id);
              setSelected(new Set(ids));
              if (ids.length) { setDryRun(false); setModal({ kind: "confirm" }); }
            }).catch(() => {});
          }
        }
        if (sum.background) {
          api.bootstrap().then((nb) => { setCandidates(nb.candidates); setSelected(new Set()); }).catch(() => {});
          setToast(`${STRINGS[lang].backgroundScan}: ${humanSize(sum.total)}`);
          window.setTimeout(() => setToast(null), 4000);
        }
      }));
      unsubs.push(await api.on<CleanStart>("clean://start", (ev) => {
        cleanRef.current = { index: ev.index, total: ev.total, label: ev.label };
        logRef.current = [...logRef.current, { text: `[${ev.index + 1}/${ev.total}] ${ev.label}`, tone: "head" }];
        setModal({ kind: "running", ...cleanRef.current, log: logRef.current });
      }));
      unsubs.push(await api.on<string>("clean://log", (line) => {
        logRef.current = [...logRef.current, { text: line }];
        setModal({ kind: "running", ...cleanRef.current, log: logRef.current });
      }));
      unsubs.push(await api.on<CleanItem>("clean://item", (it) => {
        logRef.current = [...logRef.current, it.ok ? { text: `  ✓ ${humanSize(it.freed)}`, tone: "ok" } : { text: `  ✗ ${it.error ?? "failed"}`, tone: "fail" }];
        setModal({ kind: "running", ...cleanRef.current, log: logRef.current });
      }));
      unsubs.push(await api.on<LaunchAction>("launch://action", (la) => runLaunch(api, la)));
      unsubs.push(await api.on<RoutineRun>("routine://done", (run) => {
        setRoutineRunning(false);
        setRoutineRuns((rs) => [...rs, run]);
        setToast(`${STRINGS[lang].routineDone}: ${run.name} · ${humanSize(run.freed)}`);
        window.setTimeout(() => setToast(null), 4000);
        api.bootstrap().then((nb) => { setHistory(nb.history); setRecovery(nb.recovery); setCandidates(nb.candidates); setScans(nb.scans); }).catch(() => {});
      }));
      unsubs.push(await api.on<void>("candidates://changed", () => {
        api.bootstrap().then((nb) => { setCandidates(nb.candidates); setConfig(nb.config); }).catch(() => {});
      }));
      unsubs.push(await api.on<CleanDone>("clean://done", (done) => {
        setModal({ kind: "done", entry: done.entry, log: logRef.current });
        setDisk(done.disk);
        setRecovery(done.recovery);
        setHistory((h) => [...h, done.entry]);
        if (done.removed_ids.length) {
          const gone = new Set(done.removed_ids);
          setCandidates((cs) => cs.filter((c) => !gone.has(c.id)));
          setSelected((sel) => new Set([...sel].filter((id) => !gone.has(id))));
          setActiveId((a) => (a && gone.has(a) ? null : a));
        }
      }));
      if (b.launch) runLaunch(api, b.launch);
      else if (!b.scanning) startScan(api, b.cached);
    })();
    return () => {
      disposed = true;
      unsubs.forEach((u) => u());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const startScan = useCallback(async (api?: Api, keepCached = false, roots?: string[]) => {
    const a = api ?? apiRef.current;
    if (!a) return;
    setScanning(true);
    if (!keepCached) setCandidates([]);
    setSelected(new Set());
    setActiveId(null);
    setProgress({ phase: "scan", message: "" });
    try {
      await a.startScan(roots && roots.length ? { roots } : undefined);
    } catch (e) {
      setScanning(false);
      showToast(String(e));
    }
  }, [showToast]);

  const runLaunch = useCallback((api: Api, la: LaunchAction) => {
    launchRef.current = la;
    setLaunch(la);
    setTab("candidates");
    setModal(null);
    startScan(api, false, [la.path]);
  }, [startScan]);

  const scanHere = useCallback((path: string) => {
    const api = apiRef.current;
    if (!api) return;
    runLaunch(api, { kind: "scan", path });
  }, [runLaunch]);

  const cleanOne = useCallback((id: string) => {
    setSelected(new Set([id]));
    setDryRun(false);
    setModal({ kind: "confirm" });
  }, []);

  const saveRoutine = useCallback(async (r: Routine) => {
    const api = apiRef.current;
    if (!api) return;
    try { setRoutines(await api.saveRoutine(r)); showToast(`${s.save} ✓`); } catch (e) { showToast(String(e)); }
  }, [s, showToast]);
  const deleteRoutine = useCallback(async (id: string) => {
    const api = apiRef.current;
    if (!api) return;
    try { setRoutines(await api.deleteRoutine(id)); } catch (e) { showToast(String(e)); }
  }, [showToast]);
  const runRoutine = useCallback(async (id: string) => {
    const api = apiRef.current;
    if (!api) return;
    try { setRoutineRunning(true); await api.runRoutine(id); showToast(s.routineRunning); } catch (e) { setRoutineRunning(false); showToast(String(e)); }
  }, [s, showToast]);
  const addTemplates = useCallback(async () => {
    const api = apiRef.current;
    if (!api) return;
    const ts = await api.routineTemplates();
    let list = routines;
    for (const t of ts) { if (!list.some((r) => r.name === t.name)) list = await api.saveRoutine({ ...t, id: "" }); }
    setRoutines(list);
  }, [routines]);
  const toggleIntegration = useCallback(async (install: boolean) => {
    const api = apiRef.current;
    if (!api) return;
    try { setIntegration(install ? await api.integrationInstall() : await api.integrationUninstall()); showToast(install ? s.integrationInstalled : s.integrationNotInstalled); } catch (e) { showToast(String(e)); }
  }, [s, showToast]);

  const toggle = useCallback((id: string) => {
    setSelected((sel) => {
      const n = new Set(sel);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  }, []);
  const selectMany = useCallback((ids: string[], on: boolean) => {
    setSelected((sel) => {
      const n = new Set(sel);
      ids.forEach((id) => (on ? n.add(id) : n.delete(id)));
      return n;
    });
  }, []);
  const exclude = useCallback(async (id: string) => {
    const api = apiRef.current;
    if (!api) return;
    const now = await api.toggleExclude(id);
    setExcluded((ex) => {
      const n = new Set(ex);
      if (now) n.add(id);
      else n.delete(id);
      return n;
    });
    if (now) setSelected((sel) => { const n = new Set(sel); n.delete(id); return n; });
    showToast(now ? s.toastExcluded : s.toastRestored);
  }, [s, showToast]);
  const reveal = useCallback((path: string) => { apiRef.current?.reveal(path).catch((e) => showToast(String(e))); }, [showToast]);

  const selectedItems = useMemo(() => candidates.filter((c) => selected.has(c.id)), [candidates, selected]);
  const selectedSize = selectedItems.reduce((a, c) => a + c.size, 0);

  const runClean = useCallback(async () => {
    const api = apiRef.current;
    if (!api || selectedItems.length === 0) return;
    logRef.current = [];
    cleanRef.current = { index: 0, total: selectedItems.length, label: selectedItems[0].label };
    setModal({ kind: "running", ...cleanRef.current, log: [] });
    try {
      await api.startClean({ ids: selectedItems.map((c) => c.id), dry_run: dryRun, mode, force });
    } catch (e) {
      setModal(null);
      showToast(String(e));
    }
  }, [selectedItems, dryRun, mode, force, showToast]);

  const undoRun = useCallback(async (index: number) => {
    const api = apiRef.current;
    if (!api) return;
    try {
      const [restored, errors] = await api.undoRun(index);
      const nb = await api.bootstrap();
      setHistory(nb.history);
      setRecovery(nb.recovery);
      showToast(`↺ ${restored} ${s.restored}${errors.length ? ` · ${errors.length} ${s.failed}` : ""}`);
      startScan(api, true);
    } catch (e) {
      showToast(String(e));
    }
  }, [s, showToast, startScan]);

  const restoreEntry = useCallback(async (id: string) => {
    const api = apiRef.current;
    if (!api) return;
    try {
      setRecovery(await api.restoreEntry(id));
      const nb = await api.bootstrap();
      setHistory(nb.history);
      showToast(`↺ ${s.restored}`);
    } catch (e) {
      showToast(String(e));
    }
  }, [s, showToast]);

  const purgeRecovery = useCallback(async (all: boolean) => {
    const api = apiRef.current;
    if (!api) return;
    try {
      setRecovery(await api.recoveryPurge(all));
      const nb = await api.bootstrap();
      setHistory(nb.history);
    } catch (e) {
      showToast(String(e));
    }
  }, [showToast]);

  const addManual = useCallback(async (path: string) => {
    const api = apiRef.current;
    if (!api) return;
    try {
      const c = await api.addManualCandidate(path);
      setCandidates((cs) => (cs.some((x) => x.id === c.id) ? cs : [...cs, c]));
      setSelected((sel) => new Set([...sel, c.id]));
      showToast(`${c.label} → ${s.selected.toLowerCase()}`);
    } catch (e) {
      showToast(String(e));
    }
  }, [s, showToast]);

  const saveConfig = useCallback(async (c: Config) => {
    const api = apiRef.current;
    if (!api) return;
    const saved = await api.saveConfig(c);
    setConfig(saved);
    setMode(saved.clean.mode ?? "recovery");
    showToast(STRINGS[saved.ui.language].settingsSaved);
  }, [showToast]);

  const exportReport = useCallback(async (format: "md" | "json" | "csv" = "md") => {
    const api = apiRef.current;
    if (!api) return;
    setExportOpen(false);
    const path = await api.saveDialog(`cleanprojects-report-${new Date().toISOString().slice(0, 10)}.${format}`);
    if (!path) return;
    const ids = selected.size ? [...selected] : [];
    await api.writeReport(path, ids, format);
    showToast(`${s.exportReport}: ${path}`);
  }, [selected, s, showToast]);

  const protectPath = useCallback(async (path: string) => {
    const api = apiRef.current;
    if (!api || !path) return;
    try {
      const cfg = await api.protectPath(path);
      setConfig(cfg);
      setCandidates((cs) => cs.filter((c) => !(c.action.type === "remove-path" && c.action.path.startsWith(path))));
      setSelected((sel) => new Set([...sel].filter((id) => candidates.find((c) => c.id === id)?.action.type !== "remove-path" || !(candidates.find((c) => c.id === id)!.action as { path: string }).path.startsWith(path))));
      showToast(`${s.protected}: ${path}`);
    } catch (e) {
      showToast(String(e));
    }
  }, [candidates, s, showToast]);

  const setTheme = useCallback(async (t: "system" | "light" | "dark") => {
    if (!config) return;
    await saveConfig({ ...config, ui: { ...config.ui, theme: t } });
  }, [config, saveConfig]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (modal) return;
      const t = e.target as HTMLElement;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT")) return;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "r") { e.preventDefault(); startScan(); }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") { e.preventDefault(); setPalette((v) => !v); return; }
      if (palette) return;
      if (e.key === "1") setTab("overview");
      if (e.key === "2") setTab("candidates");
      if (e.key === "3") setTab("git");
      if (e.key === "4") setTab("explore");
      if (e.key === "5") setTab("routines");
      if (e.key === "6") setTab("history");
      if (e.key === "7") setTab("settings");
      if (e.key === "Escape") { setActiveId(null); setShowKeys(false); }
      if (e.key === "?") setShowKeys((v) => !v);
      if (e.key === "/" && tab === "candidates") { e.preventDefault(); (document.getElementById("candidate-search") as HTMLInputElement | null)?.focus(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [modal, startScan, tab, palette]);

  if (!boot || !config) {
    return <div className="shell" style={{ placeItems: "center", display: "grid" }}><Loader2 className="spin" size={28} style={{ color: "var(--ember)" }} /></div>;
  }

  const activeCount = candidates.filter((c) => !excluded.has(c.id)).length;
  const gitCount = candidates.filter((c) => c.category === "git" && !excluded.has(c.id)).length;
  const nav: Array<{ id: Tab; label: string; icon: JSX.Element; count?: number }> = [
    { id: "overview", label: s.overview, icon: <LayoutDashboard size={19} /> },
    { id: "candidates", label: s.candidates, icon: <ListChecks size={19} />, count: activeCount },
    { id: "git", label: s.git, icon: <GitBranch size={19} />, count: gitCount },
    { id: "explore", label: s.explore, icon: <LayoutGrid size={19} /> },
    { id: "routines", label: s.routines, icon: <CalendarClock size={19} />, count: routines.filter((r) => r.enabled).length },
    { id: "history", label: s.history, icon: <HistoryIcon size={19} />, count: recovery.items || history.length },
    { id: "settings", label: s.settings, icon: <SettingsIcon size={19} /> },
  ];

  const lowIds = candidates.filter((c) => c.risk === "low" && !excluded.has(c.id)).map((c) => c.id);
  const paletteActions: PaletteAction[] = [
    { id: "rescan", label: s.rescan, hint: "⌘R", icon: <RefreshCw size={14} />, run: () => startScan() },
    { id: "safe", label: lang === "tr" ? "Güvenli süpürme seç" : "Select safe sweep", hint: `${lowIds.length} ${s.items}`, icon: <Zap size={14} />, run: () => selectMany(lowIds, true) },
    { id: "clean", label: s.clean, hint: selected.size ? humanSize(selectedSize) : s.noneSelected, icon: <Trash2 size={14} />, run: () => { if (selected.size) { setModal({ kind: "confirm" }); setDryRun(false); } } },
    { id: "clear", label: s.clearSelection, icon: <X size={14} />, run: () => selectMany([...selected], false) },
    { id: "overview", label: s.overview, hint: "1", icon: <LayoutDashboard size={14} />, run: () => setTab("overview") },
    { id: "candidates", label: s.candidates, hint: "2", icon: <ListChecks size={14} />, run: () => setTab("candidates") },
    { id: "git", label: s.git, hint: "3", icon: <GitBranch size={14} />, run: () => setTab("git") },
    { id: "explore", label: s.explore, hint: "4", icon: <LayoutGrid size={14} />, run: () => setTab("explore") },
    { id: "routines", label: s.routines, hint: "5", icon: <CalendarClock size={14} />, run: () => setTab("routines") },
    { id: "history", label: s.history, hint: "6", icon: <HistoryIcon size={14} />, run: () => setTab("history") },
    { id: "settings", label: s.settings, hint: "7", icon: <SettingsIcon size={14} />, run: () => setTab("settings") },
    { id: "integration", label: integration.installed ? s.integrationRemove : s.integrationInstall, icon: <MousePointerClick size={14} />, run: () => toggleIntegration(!integration.installed) },
    { id: "export-md", label: `${s.exportAs} Markdown`, icon: <FileText size={14} />, run: () => exportReport("md") },
    { id: "export-json", label: `${s.exportAs} JSON`, icon: <FileJson size={14} />, run: () => exportReport("json") },
    { id: "export-csv", label: `${s.exportAs} CSV`, icon: <FileSpreadsheet size={14} />, run: () => exportReport("csv") },
    { id: "theme-system", label: `${s.theme}: ${s.themeSystem}`, icon: <SunMoon size={14} />, run: () => setTheme("system") },
    { id: "theme-light", label: `${s.theme}: ${s.themeLight}`, icon: <Sun size={14} />, run: () => setTheme("light") },
    { id: "theme-dark", label: `${s.theme}: ${s.themeDark}`, icon: <Moon size={14} />, run: () => setTheme("dark") },
    { id: "keys", label: lang === "tr" ? "Klavye kısayolları" : "Keyboard shortcuts", hint: "?", icon: <Keyboard size={14} />, run: () => setShowKeys(true) },
  ];

  const shortcuts: Array<[string, string]> = [
    ["⌘/Ctrl K", s.palette],
    ["1 … 7", lang === "tr" ? "görünüm değiştir" : "switch view"],
    ["⌘/Ctrl R", s.rescan],
    ["/", lang === "tr" ? "ara" : "search"],
    ["Esc", lang === "tr" ? "detayı kapat" : "close detail"],
    ["?", lang === "tr" ? "bu liste" : "this list"],
    [lang === "tr" ? "çift tık" : "double-click", lang === "tr" ? "detay" : "details"],
  ];

  return (
    <div className="shell">
      <nav className="rail" aria-label="Navigation">
        <span className="rail-logo" title={`CleanProjects v${boot.version}`}><Flame size={22} /></span>
        {nav.map((n) => (
          <button type="button" key={n.id} className={`rail-btn ${tab === n.id ? "active" : ""}`} onClick={() => setTab(n.id)} aria-current={tab === n.id ? "page" : undefined}>
            {n.icon}{n.label}{n.count !== undefined && n.count > 0 && <span className="dot">{n.count}</span>}
          </button>
        ))}
        <span className="rail-spacer" />
        <div className="rail-foot">
          <button type="button" className="ib" title={`${s.palette} (⌘K)`} onClick={() => setPalette(true)}><Command size={16} /></button>
          <button type="button" className="ib" title={s.theme} onClick={() => setTheme(theme === "system" ? "light" : theme === "light" ? "dark" : "system")}>{theme === "system" ? <SunMoon size={16} /> : theme === "light" ? <Sun size={16} /> : <Moon size={16} />}</button>
          <button type="button" className="ib" title={lang === "tr" ? "Klavye kısayolları" : "Keyboard shortcuts"} onClick={() => setShowKeys(true)}><Keyboard size={16} /></button>
          <a className="ib" href="https://github.com/furkanmeclis/clean-projects" target="_blank" rel="noreferrer" title="GitHub"><Github size={16} /></a>
          <span className="ver">v{boot.version}</span>
        </div>
      </nav>
      <main className="main">
        {update && (
          <div className="update">
            <ArrowUpCircle size={16} style={{ color: "var(--ember)" }} /> {s.updateAvailable}: v{update.version}
            <span className="spacer" />
            <button type="button" className="btn sm ember" onClick={async () => { try { await update.install(); await apiRef.current?.relaunch(); } catch (e) { showToast(String(e)); } }}>{s.installUpdate}</button>
            <button type="button" className="btn sm quiet" onClick={() => setUpdate(null)}>{s.later}</button>
          </div>
        )}
        {launch && (
          <div className="launch"><MousePointerClick size={15} /> {launch.kind === "clean" ? s.launchClean : s.launchScan}: <code>{launch.path}</code></div>
        )}
        <header className="cmdbar">
          <div className="title">{nav.find((n) => n.id === tab)?.icon}<h1>{nav.find((n) => n.id === tab)?.label}</h1></div>
          {scanning && progress && <div className="scanline"><Loader2 size={13} className="spin" style={{ color: "var(--ember)" }} /><span className="phase">{progress.phase}</span><span className="msg">{progress.message}</span></div>}
          {scanning && cached && <div className="scanline"><span>{s.cachedResult}</span></div>}
          {!scanning && lastScan && <div className="scanline ok"><CircleCheckBig size={13} /><span>{s.tabSummary} <b className="num">{lastScan.candidates}</b> {s.items} · <b className="num">{humanSize(lastScan.total)}</b> · {lastScan.projects} {s.projectsScanned} · {lastScan.repos} {s.reposScanned}{lastScan.guarded > 0 && <> · <ShieldCheck size={12} style={{ verticalAlign: -2 }} /> {lastScan.guarded} {s.guarded}</>} · {(lastScan.duration_ms / 1000).toFixed(1)}s</span></div>}
          <span className="spacer" />
          <div className="menu">
            <button type="button" className="btn quiet" onClick={() => setExportOpen(!exportOpen)} disabled={!hasScanned} title={s.exportReport}><FileDown size={16} /><ChevronDown size={12} /></button>
            {exportOpen && (
              <div className="menu-list right" onMouseLeave={() => setExportOpen(false)}>
                <button type="button" onClick={() => exportReport("md")}><FileText size={13} /> Markdown</button>
                <button type="button" onClick={() => exportReport("json")}><FileJson size={13} /> JSON</button>
                <button type="button" onClick={() => exportReport("csv")}><FileSpreadsheet size={13} /> CSV</button>
              </div>
            )}
          </div>
          <button type="button" className="btn" onClick={() => startScan()} disabled={scanning}>{scanning ? <Loader2 size={15} className="spin" /> : <RefreshCw size={15} />} {scanning ? s.scanning : hasScanned ? s.rescan : s.scan}</button>
          <button type="button" className="btn ember" disabled={selected.size === 0 || scanning} onClick={() => { setModal({ kind: "confirm" }); setDryRun(false); }} title={selected.size === 0 ? s.noneSelected : undefined}>
            <Trash2 size={15} /> {s.clean}{selected.size > 0 && <span className="num"> · {humanSize(selectedSize)}</span>}
          </button>
        </header>
        <section className="content">
          {tab === "overview" && <Overview lang={lang} boot={boot} disk={disk} candidates={candidates} excluded={excluded} selected={selected} volumes={volumes} recovery={recovery} guarded={guarded} scans={scans} onPick={(c) => { setActiveId(c.id); setTab(c.category === "git" ? "git" : "candidates"); }} onPreset={(ids) => { selectMany(ids, true); showToast(`${ids.length} ${s.items} ✓`); }} onExplore={(p) => { setExploreStart(p); setTab("explore"); }} />}
          {tab === "explore" && apiRef.current && <Explore key={exploreStart ?? "home"} lang={lang} api={apiRef.current} home={exploreStart ?? boot.home} volumes={volumes} onAdd={addManual} onProtect={protectPath} onReveal={reveal} onScanHere={scanHere} />}
          {tab === "candidates" && <Candidates lang={lang} home={boot.home} candidates={candidates} excluded={excluded} selected={selected} activeId={activeId} onToggle={toggle} onSelectMany={selectMany} onActivate={setActiveId} onExclude={exclude} onReveal={reveal} onProtect={protectPath} onCleanOne={cleanOne} hasScanned={hasScanned} scanning={scanning} />}
          {tab === "routines" && <Routines lang={lang} routines={routines} runs={routineRuns} running={routineRunning} onSave={saveRoutine} onDelete={deleteRoutine} onRun={runRoutine} onTemplates={addTemplates} />}
          {tab === "git" && <GitView lang={lang} home={boot.home} candidates={candidates} excluded={excluded} selected={selected} gitEnabled={config.git.enabled} onToggle={toggle} onActivate={(id) => { setActiveId(id); setTab("candidates"); }} onCleanOne={cleanOne} />}
          {tab === "history" && <History lang={lang} home={boot.home} history={history} recovery={recovery} onClear={async () => { await apiRef.current?.clearHistory(); setHistory([]); }} onUndo={undoRun} onRestore={restoreEntry} onPurge={purgeRecovery} />}
          {tab === "settings" && <Settings lang={lang} config={config} configPath={boot.config_path} onSave={saveConfig} integration={integration} onIntegration={toggleIntegration} />}
        </section>
      </main>
      {modal && (
        <CleanModal lang={lang} items={selectedItems} phase={modal} mode={mode} dryRun={dryRun} force={force} onMode={setMode} onDryRun={setDryRun} onForce={setForce} onRun={runClean} onClose={() => { setModal(null); setForce(false); }} />
      )}
      {palette && (
        <CommandPalette lang={lang} home={boot.home} actions={paletteActions} candidates={candidates.filter((c) => !excluded.has(c.id))} onPick={(c) => { setActiveId(c.id); setTab(c.category === "git" ? "git" : "candidates"); }} onClose={() => setPalette(false)} />
      )}
      {showKeys && (
        <div className="backdrop" onClick={() => setShowKeys(false)}>
          <div className="modal" style={{ width: 520 }} onClick={(e) => e.stopPropagation()}>
            <div className="m-h"><span className="mi"><Keyboard size={17} /></span><h2>{lang === "tr" ? "Klavye kısayolları" : "Keyboard shortcuts"}</h2><button type="button" className="ib" style={{ marginLeft: "auto" }} onClick={() => setShowKeys(false)}><X size={14} /></button></div>
            <div className="m-b"><div className="shortcuts">{shortcuts.map(([k, v]) => <div key={k}><span>{v}</span><span className="kbd">{k}</span></div>)}</div></div>
          </div>
        </div>
      )}
      {toast && <div className="toast"><Sparkles size={14} />{toast}</div>}
    </div>
  );
}
