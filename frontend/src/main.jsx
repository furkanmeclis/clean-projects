import React, { useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  AreaChart as AreaIcon,
  ArchiveX,
  Check,
  ChevronRight,
  Clock3,
  Command,
  EyeOff,
  Flame,
  FolderSearch,
  Globe2,
  HardDrive,
  History,
  Loader2,
  Power,
  RefreshCw,
  RotateCcw,
  Search,
  ShieldCheck,
  Sparkles,
  Terminal,
  Trash2,
  X,
} from "lucide-react";
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import "./styles.css";

const developer = {
  name: "Furkan Meclis",
  handle: "@furkanmeclis",
  url: "https://github.com/furkanmeclis",
};

const copy = {
  tr: {
    product: "CleanProjects",
    badge: "macOS developer cleanup",
    title: "Disk analizi ve güvenli temizlik konsolu",
    subtitle: "Docker, Xcode, Cursor, Claude, Chrome ve proje artefact'larını tek ekranda analiz et. Silinecek her şey önce seçilir, risklenir ve iCloud yolları korunur.",
    protected: "iCloud Drive ve Library/Mobile Documents kilitli koruma listesinde.",
    scan: "Yeniden tara",
    delete: "Temizliği başlat",
    shutdown: "Uygulamayı kapat",
    terminal: "İşlem günlüğü",
    terminalHint: "Silme işlemi sırasında çalışan komutları ve sonuçları buradan izleyebilirsin.",
    cancel: "Vazgeç",
    confirmDelete: "Seçili temizlik çalıştırılsın mı?",
    confirmBody: "Bu işlem geri alınamaz. Yüksek riskli uygulama verilerini temizlemeden önce ilgili uygulamayı kapatman önerilir.",
    free: "Boş alan",
    recoverable: "Kazanılabilir alan",
    selected: "Seçili",
    ignored: "Saf dışı",
    overview: "Genel Bakış",
    candidates: "Adaylar",
    history: "Geçmiş",
    settings: "Güvenlik",
    search: "Docker, Cursor, Chrome, node_modules...",
    active: "Aktif",
    excluded: "Saf dışı",
    all: "Tümü",
    storageTrend: "Depolama Profili",
    largest: "En büyük alanlar",
    distribution: "Tür dağılımı",
    cleanupQueue: "Temizlik adayları",
    path: "Yol / Komut",
    kind: "Tür",
    risk: "Risk",
    size: "Boyut",
    action: "Aksiyon",
    exclude: "Saf dışı",
    restore: "Geri al",
    clean: "Temiz",
    noHistory: "Henüz temizlik yapılmadı.",
    done: "Temizlik tamamlandı.",
    shutdownDone: "Uygulama kapatılıyor. Bu sekmeyi kapatabilirsin.",
    analyzingTitle: "Disk haritası çıkarılıyor",
    analyzingBody: "Geliştirici cache'leri, proje çıktıları ve mobil runtime alanları güvenlik kurallarıyla ölçülüyor.",
    step1: "iCloud yolları korunuyor",
    step2: "Aday klasörler ölçülüyor",
    step3: "Risk etiketleri hazırlanıyor",
    by: "Geliştirici",
  },
  en: {
    product: "CleanProjects",
    badge: "macOS developer cleanup",
    title: "Disk analysis and safe cleanup console",
    subtitle: "Analyze Docker, Xcode, Cursor, Claude, Chrome, and project artifacts in one premium local dashboard. Everything is selected first, risk-labeled, and iCloud-safe.",
    protected: "iCloud Drive and Library/Mobile Documents are locked in the protection list.",
    scan: "Scan again",
    delete: "Start cleanup",
    shutdown: "Quit app",
    terminal: "Activity log",
    terminalHint: "Watch cleanup commands and results while the job is running.",
    cancel: "Cancel",
    confirmDelete: "Run selected cleanup?",
    confirmBody: "This cannot be undone. Close related apps before deleting high-risk app data.",
    free: "Free space",
    recoverable: "Recoverable space",
    selected: "Selected",
    ignored: "Excluded",
    overview: "Overview",
    candidates: "Candidates",
    history: "History",
    settings: "Safety",
    search: "Docker, Cursor, Chrome, node_modules...",
    active: "Active",
    excluded: "Excluded",
    all: "All",
    storageTrend: "Storage Profile",
    largest: "Largest areas",
    distribution: "Type distribution",
    cleanupQueue: "Cleanup queue",
    path: "Path / Command",
    kind: "Kind",
    risk: "Risk",
    size: "Size",
    action: "Action",
    exclude: "Exclude",
    restore: "Restore",
    clean: "Clean",
    noHistory: "No cleanup history yet.",
    done: "Cleanup complete.",
    shutdownDone: "App is shutting down. You can close this tab.",
    analyzingTitle: "Mapping your disk",
    analyzingBody: "Developer caches, project outputs, and mobile runtime areas are measured with safety rules.",
    step1: "Protecting iCloud paths",
    step2: "Measuring candidates",
    step3: "Preparing risk labels",
    by: "Developer",
  },
};

const chartColors = ["#f97316", "#fb923c", "#ea580c", "#c2410c", "#fdba74", "#9a3412"];

function App() {
  const [scan, setScan] = useState(null);
  const [selected, setSelected] = useState(new Set());
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [view, setView] = useState("active");
  const [dialogOpen, setDialogOpen] = useState(false);
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [terminalLines, setTerminalLines] = useState([]);
  const [notice, setNotice] = useState("");
  const [lang, setLang] = useState(localStorage.getItem("cleanProjectsLang") || "tr");
  const t = copy[lang];

  const load = async () => {
    setBusy(true);
    setNotice("");
    const res = await fetch("/api/scan");
    setScan(await res.json());
    setBusy(false);
  };

  useEffect(() => {
    load();
  }, []);

  const candidates = scan?.candidates || [];
  const activeItems = candidates.filter((c) => !c.excluded && c.available);
  const selectedItems = candidates.filter((c) => selected.has(c.id) && c.available && !c.excluded);
  const selectedBytes = selectedItems.reduce((sum, c) => sum + c.sizeBytes, 0);
  const totalBytes = activeItems.reduce((sum, c) => sum + c.sizeBytes, 0);
  const history = scan?.state?.history || [];

  const visibleItems = useMemo(() => {
    const q = query.trim().toLowerCase();
    return candidates
      .filter((c) => (view === "active" ? !c.excluded : view === "excluded" ? c.excluded : true))
      .filter((c) => `${c.label} ${c.path} ${c.kind} ${c.description}`.toLowerCase().includes(q));
  }, [candidates, query, view]);

  const distribution = useMemo(() => {
    const grouped = new Map();
    for (const item of activeItems) grouped.set(item.kind, (grouped.get(item.kind) || 0) + item.sizeBytes);
    return [...grouped.entries()].map(([name, value]) => ({ name, value, label: human(value) }));
  }, [candidates]);

  const largest = activeItems.slice().sort((a, b) => b.sizeBytes - a.sizeBytes).slice(0, 8);
  const profile = largest.map((item, index) => ({
    name: item.label.length > 18 ? `${item.label.slice(0, 18)}...` : item.label,
    size: item.sizeBytes,
    index: index + 1,
  }));

  const setLanguage = (next) => {
    setLang(next);
    localStorage.setItem("cleanProjectsLang", next);
  };

  const toggleSelected = (item) => {
    if (!item.available || item.excluded) return;
    const next = new Set(selected);
    next.has(item.id) ? next.delete(item.id) : next.add(item.id);
    setSelected(next);
  };

  const toggleExclude = async (item, excluded) => {
    setBusy(true);
    const res = await fetch("/api/exclusion", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id: item.id, excluded }),
    });
    const data = await res.json();
    setScan(data.scan);
    const next = new Set(selected);
    next.delete(item.id);
    setSelected(next);
    setBusy(false);
  };

  const runDelete = async () => {
    setDialogOpen(false);
    setTerminalOpen(true);
    setTerminalLines([]);
    setBusy(true);
    const res = await fetch("/api/delete", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ids: selectedItems.map((x) => x.id) }),
    });
    const data = await res.json();
    const events = new EventSource(`/api/jobs/${data.jobId}/events`);
    events.onmessage = (event) => {
      if (!event.data) return;
      const payload = JSON.parse(event.data);
      setTerminalLines((lines) => [...lines, payload]);
      if (payload.type === "done") {
        events.close();
        setSelected(new Set());
        load();
        setNotice(t.done);
        setBusy(false);
      }
    };
    events.addEventListener("info", appendTerminalLine);
    events.addEventListener("start", appendTerminalLine);
    events.addEventListener("output", appendTerminalLine);
    events.addEventListener("success", appendTerminalLine);
    events.addEventListener("error", appendTerminalLine);
    events.addEventListener("done", (event) => {
      appendTerminalLine(event);
      events.close();
      setSelected(new Set());
      load();
      setNotice(t.done);
      setBusy(false);
    });
    events.onerror = () => {
      events.close();
      setBusy(false);
    };
  };

  const appendTerminalLine = (event) => {
    if (!event.data) return;
    setTerminalLines((lines) => [...lines, JSON.parse(event.data)]);
  };

  const shutdownApp = async () => {
    await fetch("/api/shutdown", { method: "POST" });
    setNotice(t.shutdownDone);
  };

  if (!scan) return <AnalysisScreen t={t} />;

  return (
    <main className="app-shell">
      <aside className="app-sidebar">
        <div className="brand-row">
          <div className="brand-icon"><Flame size={20} /></div>
          <div>
            <strong>{t.product}</strong>
            <span>{t.badge}</span>
          </div>
        </div>

        <nav className="nav-list">
          <a className="active"><AreaIcon size={16} /> {t.overview}</a>
          <a><FolderSearch size={16} /> {t.candidates}</a>
          <a><History size={16} /> {t.history}</a>
          <a><ShieldCheck size={16} /> {t.settings}</a>
        </nav>

        <div className="safety-card">
          <ShieldCheck size={18} />
          <p>{t.protected}</p>
        </div>

        <div className="developer-card">
          <span>{t.by}</span>
          <strong>{developer.name}</strong>
          <a href={developer.url}>{developer.handle}</a>
        </div>
      </aside>

      <section className="app-main">
        <header className="site-header">
          <div>
            <div className="breadcrumb"><Command size={14} /> {t.product} <ChevronRight size={14} /> {t.overview}</div>
            <h1>{t.title}</h1>
            <p>{t.subtitle}</p>
          </div>
          <div className="header-actions">
            <Button variant="outline" onClick={() => setLanguage(lang === "tr" ? "en" : "tr")}><Globe2 size={16} /> {lang.toUpperCase()}</Button>
            <Button variant="outline" onClick={shutdownApp}><Power size={16} /> {t.shutdown}</Button>
            <Button onClick={load} disabled={busy}><RefreshCw size={16} /> {t.scan}</Button>
          </div>
        </header>

        <section className="section-cards">
          <Metric icon={<HardDrive />} label={t.free} value={scan.disk.free} sub={`${scan.disk.pct} used`} trend="+ ready" />
          <Metric icon={<Sparkles />} label={t.recoverable} value={human(totalBytes)} sub={`${activeItems.length} items`} trend="scan" />
          <Metric icon={<Check />} label={t.selected} value={human(selectedBytes)} sub={`${selectedItems.length} items`} trend="queue" />
          <Metric icon={<ArchiveX />} label={t.ignored} value={candidates.filter((c) => c.excluded).length} sub={t.clean} trend="safe" />
        </section>

        <section className="chart-grid">
          <Card title={t.storageTrend} icon={<AreaIcon size={16} />} className="wide">
            <div className="area-chart">
              <ResponsiveContainer>
                <AreaChart data={profile} margin={{ left: 4, right: 18, top: 16, bottom: 4 }}>
                  <defs>
                    <linearGradient id="orangeArea" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="5%" stopColor="#f97316" stopOpacity={0.35} />
                      <stop offset="95%" stopColor="#f97316" stopOpacity={0.02} />
                    </linearGradient>
                  </defs>
                  <CartesianGrid strokeDasharray="3 3" vertical={false} stroke="#f1e7dd" />
                  <XAxis dataKey="index" tickLine={false} axisLine={false} tick={{ fill: "#78716c", fontSize: 12 }} />
                  <YAxis tickLine={false} axisLine={false} tick={{ fill: "#78716c", fontSize: 12 }} tickFormatter={human} width={64} />
                  <Tooltip formatter={(v) => human(v)} labelFormatter={(v) => `#${v}`} />
                  <Area type="monotone" dataKey="size" stroke="#f97316" strokeWidth={3} fill="url(#orangeArea)" />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </Card>

          <Card title={t.distribution} icon={<Flame size={16} />}>
            <div className="donut">
              <ResponsiveContainer>
                <PieChart>
                  <Pie data={distribution} dataKey="value" nameKey="name" innerRadius={58} outerRadius={82} strokeWidth={0}>
                    {distribution.map((_, i) => <Cell key={i} fill={chartColors[i % chartColors.length]} />)}
                  </Pie>
                  <Tooltip formatter={(v) => human(v)} />
                </PieChart>
              </ResponsiveContainer>
            </div>
            <div className="legend-list">
              {distribution.slice(0, 5).map((item, i) => (
                <span key={item.name}><i style={{ background: chartColors[i % chartColors.length] }} />{item.name}<b>{item.label}</b></span>
              ))}
            </div>
          </Card>
        </section>

        <section className="table-layout">
          <Card title={t.cleanupQueue} icon={<Search size={16} />} className="table-card">
            <div className="table-toolbar">
              <label className="search-field"><Search size={15} /><input value={query} onChange={(e) => setQuery(e.target.value)} placeholder={t.search} /></label>
              <Segmented value={view} onChange={setView} items={[[ "active", t.active ], [ "excluded", t.excluded ], [ "all", t.all ]]} />
              <Button variant="destructive" disabled={!selectedItems.length || busy} onClick={() => setDialogOpen(true)}><Trash2 size={16} /> {t.delete}</Button>
            </div>

            <div className="data-table">
              <div className="table-head">
                <span></span>
                <span>{t.candidates}</span>
                <span>{t.kind}</span>
                <span>{t.risk}</span>
                <span>{t.size}</span>
                <span>{t.action}</span>
              </div>
              {visibleItems.map((item) => (
                <CandidateRow
                  key={item.id}
                  item={item}
                  selected={selected.has(item.id)}
                  onSelect={() => toggleSelected(item)}
                  onExclude={() => toggleExclude(item, !item.excluded)}
                  t={t}
                />
              ))}
            </div>
          </Card>

          <Card title={t.history} icon={<Clock3 size={16} />} className="history-card">
            {history.length === 0 && <p className="empty">{t.noHistory}</p>}
            {history.map((item, index) => (
              <div className="history-row" key={`${item.time}-${index}`}>
                <div>
                  <strong>{item.totalHuman}</strong>
                  <span>{new Date(item.time).toLocaleString(lang === "tr" ? "tr-TR" : "en-US")}</span>
                </div>
                <small>{item.items.map((x) => x.label).slice(0, 2).join(", ")}</small>
              </div>
            ))}
          </Card>
        </section>
      </section>

      {dialogOpen && (
        <Dialog onClose={() => setDialogOpen(false)}>
          <div className="dialog-icon"><Trash2 size={22} /></div>
          <h2>{t.confirmDelete}</h2>
          <p>{t.confirmBody}</p>
          <div className="dialog-summary"><span>{selectedItems.length} items</span><strong>{human(selectedBytes)}</strong></div>
          <div className="dialog-actions">
            <Button variant="outline" onClick={() => setDialogOpen(false)}>{t.cancel}</Button>
            <Button variant="destructive" onClick={runDelete}><Trash2 size={16} /> {t.delete}</Button>
          </div>
        </Dialog>
      )}

      {terminalOpen && (
        <Dialog onClose={() => !busy && setTerminalOpen(false)} wide>
          <div className="terminal-header">
            <div className="dialog-icon"><Terminal size={22} /></div>
            <div>
              <h2>{t.terminal}</h2>
              <p>{t.terminalHint}</p>
            </div>
          </div>
          <div className="terminal-panel">
            {terminalLines.length === 0 && <pre><span>00:00:00</span> waiting for cleanup job...</pre>}
            {terminalLines.map((line, index) => (
              <pre key={`${line.time}-${index}`} className={`term-${line.type}`}>
                <span>{line.time}</span> {line.label ? `[${line.label}] ` : ""}{line.message}{line.size ? ` (${line.size})` : ""}
              </pre>
            ))}
          </div>
          <div className="dialog-actions">
            <Button variant="outline" disabled={busy} onClick={() => setTerminalOpen(false)}>{t.cancel}</Button>
          </div>
        </Dialog>
      )}

      {busy && <div className="floating-status"><Loader2 className="spin" size={16} /> {t.step2}</div>}
      {notice && <div className="toast"><Check size={16} /> {notice}</div>}
    </main>
  );
}

function AnalysisScreen({ t }) {
  return (
    <main className="analysis-screen">
      <section className="analysis-panel">
        <div className="analysis-mark"><Flame size={34} /></div>
        <span>{t.badge}</span>
        <h1>{t.analyzingTitle}</h1>
        <p>{t.analyzingBody}</p>
        <div className="analysis-steps">
          <b><Check size={15} /> {t.step1}</b>
          <b><Loader2 className="spin" size={15} /> {t.step2}</b>
          <b><ChevronRight size={15} /> {t.step3}</b>
        </div>
      </section>
    </main>
  );
}

function CandidateRow({ item, selected, onSelect, onExclude, t }) {
  return (
    <div className={`table-row risk-${item.risk} ${selected ? "selected" : ""} ${item.excluded ? "excluded" : ""}`} onClick={onSelect}>
      <span className="select-control">{selected && <Check size={13} />}</span>
      <div className="candidate-cell">
        <strong>{item.label}</strong>
        <code>{item.path || item.method}</code>
      </div>
      <span className="pill">{item.kind}</span>
      <span className="risk-pill">{item.risk}</span>
      <strong className="size-cell">{item.sizeHuman}</strong>
      <button className="row-action" disabled={!item.available} onClick={(event) => { event.stopPropagation(); onExclude(); }}>
        {item.excluded ? <RotateCcw size={14} /> : <EyeOff size={14} />}
        {item.excluded ? t.restore : t.exclude}
      </button>
    </div>
  );
}

function Metric({ icon, label, value, sub, trend }) {
  return (
    <article className="metric-card">
      <div className="metric-top">{React.cloneElement(icon, { size: 18 })}<span>{trend}</span></div>
      <p>{label}</p>
      <strong>{value}</strong>
      <small>{sub}</small>
    </article>
  );
}

function Card({ title, icon, children, className = "" }) {
  return <section className={`card ${className}`}><div className="card-header">{icon}<strong>{title}</strong></div>{children}</section>;
}

function Button({ children, variant = "primary", ...props }) {
  return <button className={`button ${variant}`} {...props}>{children}</button>;
}

function Segmented({ value, onChange, items }) {
  return <div className="segmented">{items.map(([id, label]) => <button key={id} className={value === id ? "active" : ""} onClick={() => onChange(id)}>{label}</button>)}</div>;
}

function Dialog({ children, onClose, wide = false }) {
  return (
    <div className="dialog-backdrop" onMouseDown={onClose}>
      <div className={`dialog ${wide ? "wide" : ""}`} onMouseDown={(event) => event.stopPropagation()}>
        <button className="dialog-close" onClick={onClose}><X size={16} /></button>
        {children}
      </div>
    </div>
  );
}

function human(n) {
  if (!n) return "0B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = n;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index++;
  }
  return `${Number(value.toFixed(1)).toLocaleString()}${units[index]}`;
}

createRoot(document.getElementById("root")).render(<App />);
