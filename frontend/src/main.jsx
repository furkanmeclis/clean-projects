import React, { useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  ArchiveX,
  Check,
  ChevronRight,
  Clock3,
  EyeOff,
  Flame,
  Globe2,
  HardDrive,
  Loader2,
  PieChart as PieIcon,
  RefreshCw,
  RotateCcw,
  Search,
  ShieldCheck,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { Bar, BarChart, Cell, Pie, PieChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import "./styles.css";

const copy = {
  tr: {
    badge: "Yerel temizlik konsolu",
    title: "CleanProjects",
    subtitle: "Diskini tarar, riskli alanları ayırır ve yalnızca seçtiğin yeniden üretilebilir verileri temizler.",
    protected: "iCloud Drive ve Library/Mobile Documents kapsam dışı. Bu yollar kilitli koruma listesinde.",
    scan: "Yeniden analiz et",
    delete: "Temizliği başlat",
    cancel: "Vazgeç",
    confirmDelete: "Seçili öğeleri temizle",
    confirmBody: "Bu işlem geri alınamaz. Yüksek riskli öğelerde uygulamayı kapatmış olman önerilir.",
    selected: "Seçili",
    recoverable: "Aktif potansiyel",
    free: "Boş alan",
    excluded: "Saf dışı",
    search: "Docker, Cursor, Chrome, node_modules...",
    active: "Aktif",
    ignored: "Saf dışı",
    all: "Tümü",
    history: "Geçmiş",
    noHistory: "Henüz temizlik yapılmadı.",
    distribution: "Dağılım",
    largest: "En büyükler",
    exclude: "Saf dışı",
    restore: "Geri al",
    done: "Temizlik tamamlandı.",
    analyzingTitle: "Disk haritası çıkarılıyor",
    analyzingBody: "Projeler, cache alanları, Docker, Cursor, Claude, Xcode ve mobil SDK klasörleri güvenli kurallarla taranıyor.",
    analyzingStep1: "iCloud yolları korunuyor",
    analyzingStep2: "Yeniden üretilebilir alanlar ölçülüyor",
    analyzingStep3: "Risk etiketleri hazırlanıyor",
    risk: "risk",
  },
  en: {
    badge: "Local cleanup console",
    title: "CleanProjects",
    subtitle: "Scans your disk, separates risky areas, and cleans only the regenerable data you choose.",
    protected: "iCloud Drive and Library/Mobile Documents are out of scope. Those paths are locked in the protection list.",
    scan: "Analyze again",
    delete: "Start cleanup",
    cancel: "Cancel",
    confirmDelete: "Clean selected items",
    confirmBody: "This cannot be undone. For high-risk app data, close the related app first.",
    selected: "Selected",
    recoverable: "Active potential",
    free: "Free space",
    excluded: "Excluded",
    search: "Docker, Cursor, Chrome, node_modules...",
    active: "Active",
    ignored: "Excluded",
    all: "All",
    history: "History",
    noHistory: "No cleanup history yet.",
    distribution: "Distribution",
    largest: "Largest",
    exclude: "Exclude",
    restore: "Restore",
    done: "Cleanup complete.",
    analyzingTitle: "Mapping your disk",
    analyzingBody: "Projects, caches, Docker, Cursor, Claude, Xcode, and mobile SDK folders are scanned with safety rules.",
    analyzingStep1: "Protecting iCloud paths",
    analyzingStep2: "Measuring regenerable storage",
    analyzingStep3: "Preparing risk labels",
    risk: "risk",
  },
};

const colors = ["#f97316", "#fb923c", "#ea580c", "#c2410c", "#fdba74", "#9a3412"];

function App() {
  const [scan, setScan] = useState(null);
  const [selected, setSelected] = useState(new Set());
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [view, setView] = useState("active");
  const [dialogOpen, setDialogOpen] = useState(false);
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
  const visibleItems = useMemo(() => {
    const q = query.trim().toLowerCase();
    return candidates
      .filter((c) => (view === "active" ? !c.excluded : view === "ignored" ? c.excluded : true))
      .filter((c) => `${c.label} ${c.path} ${c.kind} ${c.description}`.toLowerCase().includes(q));
  }, [candidates, query, view]);

  const activeItems = candidates.filter((c) => !c.excluded && c.available);
  const selectedItems = candidates.filter((c) => selected.has(c.id) && c.available && !c.excluded);
  const selectedBytes = selectedItems.reduce((sum, c) => sum + c.sizeBytes, 0);
  const totalBytes = activeItems.reduce((sum, c) => sum + c.sizeBytes, 0);
  const history = scan?.state?.history || [];

  const distribution = useMemo(() => {
    const grouped = new Map();
    for (const item of activeItems) grouped.set(item.kind, (grouped.get(item.kind) || 0) + item.sizeBytes);
    return [...grouped.entries()].map(([name, value]) => ({ name, value, label: human(value) }));
  }, [candidates]);

  const largest = activeItems.slice().sort((a, b) => b.sizeBytes - a.sizeBytes).slice(0, 7);

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
    setBusy(true);
    const res = await fetch("/api/delete", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ids: selectedItems.map((x) => x.id) }),
    });
    const data = await res.json();
    setSelected(new Set());
    setScan(data.scan);
    setNotice(t.done);
    setBusy(false);
  };

  if (!scan) return <AnalysisScreen t={t} />;

  return (
    <main>
      <section className="shell">
        <aside className="sidebar">
          <div className="brand-mark"><Flame size={22} /></div>
          <div>
            <div className="overline">{t.badge}</div>
            <h1>{t.title}</h1>
          </div>
          <p>{t.subtitle}</p>
          <div className="safe-note"><ShieldCheck size={18} /> {t.protected}</div>
          <div className="sidebar-actions">
            <Button variant="secondary" onClick={() => setLanguage(lang === "tr" ? "en" : "tr")}><Globe2 size={16} /> {lang.toUpperCase()}</Button>
            <Button onClick={load} disabled={busy}><RefreshCw size={16} /> {t.scan}</Button>
          </div>
        </aside>

        <section className="content">
          <div className="top-grid">
            <Metric icon={<HardDrive />} label={t.free} value={scan.disk.free} sub={`${scan.disk.pct} used`} />
            <Metric icon={<Sparkles />} label={t.recoverable} value={human(totalBytes)} sub={`${activeItems.length} items`} />
            <Metric icon={<Check />} label={t.selected} value={human(selectedBytes)} sub={`${selectedItems.length} items`} />
            <Metric icon={<ArchiveX />} label={t.excluded} value={candidates.filter((c) => c.excluded).length} sub="hidden" />
          </div>

          <div className="insights">
            <Panel title={t.distribution} icon={<PieIcon size={17} />}>
              <div className="donut-wrap">
                <ResponsiveContainer>
                  <PieChart>
                    <Pie data={distribution} dataKey="value" nameKey="name" innerRadius={58} outerRadius={86} strokeWidth={0}>
                      {distribution.map((_, i) => <Cell key={i} fill={colors[i % colors.length]} />)}
                    </Pie>
                    <Tooltip formatter={(v) => human(v)} />
                  </PieChart>
                </ResponsiveContainer>
              </div>
              <div className="legend">
                {distribution.map((x, i) => <span key={x.name}><i style={{ background: colors[i % colors.length] }} />{x.name} · {x.label}</span>)}
              </div>
            </Panel>

            <Panel title={t.largest} icon={<Flame size={17} />}>
              <div className="bar-wrap">
                <ResponsiveContainer>
                  <BarChart data={largest} layout="vertical" margin={{ left: 8, right: 12, top: 2, bottom: 2 }}>
                    <XAxis type="number" hide />
                    <YAxis type="category" dataKey="label" width={116} tick={{ fontSize: 11, fill: "#78716c" }} />
                    <Tooltip formatter={(v) => human(v)} />
                    <Bar dataKey="sizeBytes" fill="#f97316" radius={[0, 7, 7, 0]} />
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </Panel>
          </div>

          <Panel className="scanner" title="Scanner" icon={<Search size={17} />}>
            <div className="toolbar">
              <div className="input-wrap"><Search size={16} /><input value={query} onChange={(e) => setQuery(e.target.value)} placeholder={t.search} /></div>
              <Segmented value={view} onChange={setView} items={[
                ["active", t.active],
                ["ignored", t.ignored],
                ["all", t.all],
              ]} />
              <Button variant="destructive" disabled={!selectedItems.length || busy} onClick={() => setDialogOpen(true)}>
                <Trash2 size={16} /> {t.delete}
              </Button>
            </div>

            <div className="candidate-list">
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
          </Panel>
        </section>

        <aside className="history-rail">
          <Panel title={t.history} icon={<Clock3 size={17} />}>
            {history.length === 0 && <p className="empty">{t.noHistory}</p>}
            {history.map((h, idx) => (
              <div className="history-item" key={`${h.time}-${idx}`}>
                <strong>{h.totalHuman}</strong>
                <span>{new Date(h.time).toLocaleString(lang === "tr" ? "tr-TR" : "en-US")}</span>
                <small>{h.items.map((x) => x.label).slice(0, 3).join(", ")}</small>
              </div>
            ))}
          </Panel>
        </aside>
      </section>

      {dialogOpen && (
        <Dialog onClose={() => setDialogOpen(false)}>
          <div className="dialog-icon"><Trash2 size={22} /></div>
          <h2>{t.confirmDelete}</h2>
          <p>{t.confirmBody}</p>
          <div className="dialog-summary">
            <span>{selectedItems.length} items</span>
            <strong>{human(selectedBytes)}</strong>
          </div>
          <div className="dialog-actions">
            <Button variant="secondary" onClick={() => setDialogOpen(false)}>{t.cancel}</Button>
            <Button variant="destructive" onClick={runDelete}><Trash2 size={16} /> {t.delete}</Button>
          </div>
        </Dialog>
      )}

      {busy && <div className="busy-pill"><Loader2 className="spin" size={16} /> {t.analyzingStep2}</div>}
      {notice && <div className="toast"><Check size={16} /> {notice}</div>}
    </main>
  );
}

function AnalysisScreen({ t }) {
  return (
    <main className="analysis-page">
      <section className="analysis-card">
        <div className="analysis-orbit"><Flame size={34} /></div>
        <div className="overline">{t.badge}</div>
        <h1>{t.analyzingTitle}</h1>
        <p>{t.analyzingBody}</p>
        <div className="analysis-steps">
          <span><Check size={15} /> {t.analyzingStep1}</span>
          <span><Loader2 className="spin" size={15} /> {t.analyzingStep2}</span>
          <span><ChevronRight size={15} /> {t.analyzingStep3}</span>
        </div>
      </section>
    </main>
  );
}

function CandidateRow({ item, selected, onSelect, onExclude, t }) {
  return (
    <article className={`candidate risk-${item.risk} ${selected ? "selected" : ""} ${item.excluded ? "excluded" : ""}`} onClick={onSelect}>
      <div className="select-dot">{selected && <Check size={14} />}</div>
      <div className="candidate-main">
        <div className="candidate-title">
          <strong>{item.label}</strong>
          <span>{item.kind}</span>
          <span>{item.risk} {t.risk}</span>
          {item.excluded && <span>{t.excluded}</span>}
        </div>
        <p>{item.description}</p>
        <code>{item.path || item.method}</code>
      </div>
      <div className="candidate-side">
        <strong>{item.sizeHuman}</strong>
        <button className="mini-button" onClick={(e) => { e.stopPropagation(); onExclude(); }} disabled={!item.available}>
          {item.excluded ? <RotateCcw size={14} /> : <EyeOff size={14} />}
          {item.excluded ? t.restore : t.exclude}
        </button>
      </div>
    </article>
  );
}

function Metric({ icon, label, value, sub }) {
  return <div className="metric">{React.cloneElement(icon, { size: 19 })}<span>{label}</span><strong>{value}</strong><small>{sub}</small></div>;
}

function Panel({ title, icon, children, className = "" }) {
  return <section className={`panel ${className}`}><div className="panel-title">{icon}{title}</div>{children}</section>;
}

function Button({ children, variant = "primary", ...props }) {
  return <button className={`btn ${variant}`} {...props}>{children}</button>;
}

function Segmented({ value, onChange, items }) {
  return <div className="segmented">{items.map(([id, label]) => <button key={id} className={value === id ? "active" : ""} onClick={() => onChange(id)}>{label}</button>)}</div>;
}

function Dialog({ children, onClose }) {
  return (
    <div className="dialog-backdrop" onMouseDown={onClose}>
      <div className="dialog" onMouseDown={(e) => e.stopPropagation()}>
        <button className="dialog-close" onClick={onClose}><X size={16} /></button>
        {children}
      </div>
    </div>
  );
}

function human(n) {
  if (!n) return "0B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n, i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return `${Number(v.toFixed(1)).toLocaleString()}${units[i]}`;
}

createRoot(document.getElementById("root")).render(<App />);
