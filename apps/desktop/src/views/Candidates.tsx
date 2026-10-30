import { useMemo, useState } from "react";
import { ArrowDownWideNarrow, ArrowUpNarrowWide, Calendar, ChevronDown, EyeOff, FileText, Fingerprint, FolderGit2, FolderOpen, FolderSearch, Hourglass, Info, Layers, ListFilter, Lock, RotateCcw, Search, ShieldPlus, Sparkles, SquareCheckBig, SquareDashed, Usb, X } from "lucide-react";
import { useEffect } from "react";
import { ContextMenu, openMenu, type MenuState } from "../components/ContextMenu";
import { Trash2, SquareCheck } from "lucide-react";
import type { Candidate, Category, Risk } from "../types";
import { CATEGORIES, actionDisplay, actionPath } from "../types";
import { ageDays, ageLabel, compactPath, formatDate, humanSize } from "../format";
import type { Lang } from "../i18n";
import { CATEGORY_LABELS, STRINGS } from "../i18n";
import { CATEGORY_ICON, CATEGORY_VAR, CategoryBadge, CheckBox, RiskPill, Tags } from "../components/common";

export type SortKey = "size" | "name" | "age" | "risk";
export type GroupKey = "none" | "category" | "owner" | "project" | "volume";

interface Props {
  lang: Lang;
  home: string;
  candidates: Candidate[];
  excluded: Set<string>;
  selected: Set<string>;
  activeId: string | null;
  onToggle: (id: string) => void;
  onSelectMany: (ids: string[], on: boolean) => void;
  onActivate: (id: string | null) => void;
  onExclude: (id: string) => void;
  onReveal: (path: string) => void;
  onProtect: (path: string) => void;
  onCleanOne: (id: string) => void;
  hasScanned: boolean;
  scanning: boolean;
}

const RISK_ORDER: Record<Risk, number> = { low: 0, medium: 1, high: 2 };

export function Candidates(p: Props) {
  const s = STRINGS[p.lang];
  const [query, setQuery] = useState("");
  const [risk, setRisk] = useState<Risk | "all">("all");
  const [category, setCategory] = useState<Category | "all">("all");
  const [catOpen, setCatOpen] = useState(false);
  const [staleOnly, setStaleOnly] = useState(false);
  const [newOnly, setNewOnly] = useState(false);
  const [cursor, setCursor] = useState<number>(-1);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const rowMenu = (e: React.MouseEvent, c: Candidate) => {
    const isSel = p.selected.has(c.id);
    const isEx = p.excluded.has(c.id);
    const path = actionPath(c.action);
    openMenu(e, [
      { label: isSel ? s.ctxDeselect : s.ctxSelect, icon: <SquareCheck size={14} />, disabled: isEx, onClick: () => p.onToggle(c.id) },
      { label: s.ctxDetails, icon: <Info size={14} />, onClick: () => p.onActivate(c.id) },
      { label: s.reveal, icon: <FolderOpen size={14} />, disabled: !path, onClick: () => path && p.onReveal(path) },
      { separator: true, label: "" },
      { label: s.ctxCleanThis, icon: <Trash2 size={14} />, disabled: isEx || !!c.blocked_by, onClick: () => p.onCleanOne(c.id) },
      { separator: true, label: "" },
      { label: isEx ? s.restore : s.exclude, icon: isEx ? <RotateCcw size={14} /> : <EyeOff size={14} />, onClick: () => p.onExclude(c.id) },
      { label: s.protect, icon: <ShieldPlus size={14} />, disabled: c.action.type !== "remove-path", onClick: () => c.action.type === "remove-path" && p.onProtect(c.action.path) },
    ], setMenu);
  };
  const [showExcluded, setShowExcluded] = useState(false);
  const [sort, setSort] = useState<SortKey>("size");
  const [desc, setDesc] = useState(true);
  const [group, setGroup] = useState<GroupKey>("none");
  const [groupOpen, setGroupOpen] = useState(false);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const rows = p.candidates.filter((c) => {
      if (!showExcluded && p.excluded.has(c.id)) return false;
      if (risk !== "all" && c.risk !== risk) return false;
      if (category !== "all" && c.category !== category) return false;
      if (staleOnly && !c.tags?.includes("stale")) return false;
      if (newOnly && !c.tags?.includes("new")) return false;
      if (q) {
        const hay = `${c.label} ${actionDisplay(c.action)} ${c.category} ${c.project?.name ?? ""} ${(c.tags ?? []).join(" ")}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });
    const dir = desc ? -1 : 1;
    rows.sort((a, b) => {
      switch (sort) {
        case "size": return (a.size - b.size) * dir;
        case "name": return a.label.localeCompare(b.label) * -dir;
        case "age": return ((ageDays(a.modified) ?? -1) - (ageDays(b.modified) ?? -1)) * dir;
        case "risk": return (RISK_ORDER[a.risk] - RISK_ORDER[b.risk]) * dir || b.size - a.size;
      }
    });
    return rows;
  }, [p.candidates, p.excluded, query, risk, category, staleOnly, newOnly, showExcluded, sort, desc]);

  // Keyboard navigation over the visible rows.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT")) return;
      if (document.querySelector(".backdrop")) return;
      if (e.key === "ArrowDown" || e.key === "j") { e.preventDefault(); setCursor((c) => Math.min(visible.length - 1, c + 1)); }
      else if (e.key === "ArrowUp" || e.key === "k") { e.preventDefault(); setCursor((c) => Math.max(0, c - 1)); }
      else if (e.key === " " && cursor >= 0 && visible[cursor]) { e.preventDefault(); if (!p.excluded.has(visible[cursor].id)) p.onToggle(visible[cursor].id); }
      else if (e.key === "Enter" && cursor >= 0 && visible[cursor]) { e.preventDefault(); p.onActivate(p.activeId === visible[cursor].id ? null : visible[cursor].id); }
      else if (e.key === "x" && cursor >= 0 && visible[cursor]) { e.preventDefault(); p.onExclude(visible[cursor].id); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [visible, cursor, p]);
  useEffect(() => {
    if (cursor < 0) return;
    const el = document.querySelector<HTMLElement>(`[data-row-index="${cursor}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  const groupKey = (c: Candidate): string => {
    switch (group) {
      case "category": return CATEGORY_LABELS[p.lang][c.category];
      case "owner": return c.owner || c.label.split(" ")[0];
      case "project": return c.project?.name ?? "—";
      case "volume": return c.volume ?? "/";
      default: return "";
    }
  };
  const grouped = useMemo(() => {
    if (group === "none") return [{ key: "", rows: visible }];
    const m = new Map<string, Candidate[]>();
    for (const c of visible) { const k = groupKey(c); if (!m.has(k)) m.set(k, []); m.get(k)!.push(c); }
    return [...m.entries()].map(([key, rows]) => ({ key, rows })).sort((a, b) => b.rows.reduce((x, c) => x + c.size, 0) - a.rows.reduce((x, c) => x + c.size, 0));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible, group, p.lang]);
  const visibleTotal = visible.reduce((a, c) => a + c.size, 0);
  const selectedVisible = visible.filter((c) => p.selected.has(c.id));
  const active = p.activeId ? p.candidates.find((c) => c.id === p.activeId) ?? null : null;
  const toggleSort = (k: SortKey) => { if (sort === k) setDesc(!desc); else { setSort(k); setDesc(true); } };
  const SortIcon = ({ k }: { k: SortKey }) => (sort === k ? (desc ? <ArrowDownWideNarrow size={12} /> : <ArrowUpNarrowWide size={12} />) : null);
  const allVisibleSelected = visible.length > 0 && visible.every((c) => p.selected.has(c.id) || p.excluded.has(c.id));

  return (
    <>
      <div className="toolbar">
        <label className="search">
          <Search size={15} />
          <input id="candidate-search" placeholder={s.search} value={query} onChange={(e) => setQuery(e.target.value)} />
          {query ? <button type="button" className="ib" onClick={() => setQuery("")}><X size={14} /></button> : <span className="kbd">/</span>}
        </label>
        <div className="seg" role="radiogroup" aria-label={s.colRisk}>
          {(["all", "low", "medium", "high"] as const).map((r) => (
            <button type="button" key={r} className={`${r === "low" ? "lo" : r === "medium" ? "me" : r === "high" ? "hi" : ""} ${risk === r ? "active" : ""}`} onClick={() => setRisk(r)}>
              {r === "all" ? s.all : r === "low" ? s.low : r === "medium" ? s.medium : s.high}
            </button>
          ))}
        </div>
        <div className="menu">
          <button type="button" className={`tog ${category !== "all" ? "on" : ""}`} onClick={() => setCatOpen(!catOpen)}>
            <ListFilter size={14} /> {category === "all" ? s.colCategory : CATEGORY_LABELS[p.lang][category]} <ChevronDown size={13} />
          </button>
          {catOpen && (
            <div className="menu-list" onMouseLeave={() => setCatOpen(false)}>
              <button type="button" className={category === "all" ? "active" : ""} onClick={() => { setCategory("all"); setCatOpen(false); }}>{s.all}</button>
              {CATEGORIES.map((c) => {
                const I = CATEGORY_ICON[c];
                const n = p.candidates.filter((x) => x.category === c && !p.excluded.has(x.id)).length;
                return (
                  <button type="button" key={c} className={category === c ? "active" : ""} onClick={() => { setCategory(c); setCatOpen(false); }} disabled={n === 0}>
                    <I size={13} style={{ color: CATEGORY_VAR[c] }} />{CATEGORY_LABELS[p.lang][c]}<span className="num" style={{ marginLeft: "auto", color: "var(--ink-3)" }}>{n}</span>
                  </button>
                );
              })}
            </div>
          )}
        </div>
        <div className="menu">
          <button type="button" className={`tog ${group !== "none" ? "on" : ""}`} onClick={() => setGroupOpen(!groupOpen)}><Layers size={14} /> {s.groupBy}{group !== "none" && `: ${({ category: s.groupCategory, owner: s.groupOwner, project: s.groupProject, volume: s.groupVolume, none: "" })[group]}`} <ChevronDown size={13} /></button>
          {groupOpen && (
            <div className="menu-list" onMouseLeave={() => setGroupOpen(false)}>
              {(["none", "category", "owner", "project", "volume"] as GroupKey[]).map((g) => (
                <button type="button" key={g} className={group === g ? "active" : ""} onClick={() => { setGroup(g); setGroupOpen(false); }}>{({ none: s.groupNone, category: s.groupCategory, owner: s.groupOwner, project: s.groupProject, volume: s.groupVolume })[g]}</button>
              ))}
            </div>
          )}
        </div>
        <button type="button" className={`tog ${staleOnly ? "on" : ""}`} onClick={() => setStaleOnly(!staleOnly)}><Hourglass size={14} /> {s.staleOnly}</button>
        {p.candidates.some((c) => c.tags?.includes("new")) && <button type="button" className={`tog ${newOnly ? "on" : ""}`} onClick={() => setNewOnly(!newOnly)}><Sparkles size={14} /> {s.newOnly}</button>}
        <button type="button" className={`tog ${showExcluded ? "on" : ""}`} onClick={() => setShowExcluded(!showExcluded)}><EyeOff size={14} /> {s.showExcluded}</button>
        <div className="right">
          <button type="button" className="btn sm" onClick={() => p.onSelectMany(visible.filter((c) => c.risk === "low" && !p.excluded.has(c.id)).map((c) => c.id), true)}><Sparkles size={13} /> {s.selectSafe}</button>
          <button type="button" className="btn sm quiet" onClick={() => p.onSelectMany([...p.selected], false)} disabled={p.selected.size === 0}><SquareDashed size={13} /> {s.clearSelection}</button>
        </div>
      </div>

      <div className={active ? "split" : ""}>
        <div className="table">
          <div className="thead">
            <button type="button" title={s.selectVisible} onClick={() => p.onSelectMany(visible.filter((c) => !p.excluded.has(c.id)).map((c) => c.id), !allVisibleSelected)} style={{ justifySelf: "center", marginLeft: 12 }}>
              <SquareCheckBig size={14} style={{ color: allVisibleSelected ? "var(--ember)" : undefined }} />
            </button>
            <span />
            <button type="button" onClick={() => toggleSort("name")}>{s.colItem} <SortIcon k="name" /></button>
            <span>{s.colCategory}</span>
            <button type="button" onClick={() => toggleSort("risk")}>{s.colRisk} <SortIcon k="risk" /></button>
            <button type="button" onClick={() => toggleSort("age")}>{s.colAge} <SortIcon k="age" /></button>
            <button type="button" className="r" onClick={() => toggleSort("size")}>{s.colSize} <SortIcon k="size" /></button>
            <span />
          </div>
          {visible.length === 0 && (
            <div className="empty">
              <span className="ei">{p.scanning ? <Search size={24} /> : <Sparkles size={24} />}</span>
              <strong>{p.scanning ? s.scanning : !p.hasScanned ? s.emptyScan : p.candidates.length === 0 ? s.tidy : s.emptyFiltered}</strong>
            </div>
          )}
          {grouped.map(({ key, rows }) => (
            <div key={key || "all"}>
            {group !== "none" && <div className="group-h">{group === "volume" ? <Usb size={13} /> : <Layers size={13} />}{key}<span className="num">{rows.length} · {humanSize(rows.reduce((a, c) => a + c.size, 0))}</span></div>}
            {rows.map((c) => {
            const isSel = p.selected.has(c.id);
            const isEx = p.excluded.has(c.id);
            const path = actionPath(c.action);
            const I = CATEGORY_ICON[c.category];
            const rowIndex = visible.indexOf(c);
            return (
              <div key={c.id} data-row-index={rowIndex} className={`trow risk-${c.risk} ${isSel ? "sel" : ""} ${isEx ? "ex" : ""} ${p.activeId === c.id || cursor === rowIndex ? "cur" : ""}`}
                onClick={() => { setCursor(rowIndex); if (!isEx) p.onToggle(c.id); }} onDoubleClick={() => p.onActivate(c.id)} onContextMenu={(e) => { setCursor(rowIndex); rowMenu(e, c); }}>
                <span onClick={(e) => { e.stopPropagation(); if (!isEx) p.onToggle(c.id); }}><CheckBox checked={isSel} /></span>
                <span className="cico" style={{ background: CATEGORY_VAR[c.category] }}><I size={15} /></span>
                <div className="cell">
                  <span className="n">{c.label} <Tags c={c} lang={p.lang} />{c.tags?.includes("new") && <span className="tag new"><Sparkles size={9} />{s.newTag}</span>}{c.blocked_by && <span className="blocked"><Lock size={9} />{c.blocked_by} · {s.blockedBy}</span>}{c.volume && c.volume !== "/" && <span className="tag mute"><Usb size={9} />{c.volume.split(/[\/]/).pop()}</span>}{isEx && <span className="tag mute"><EyeOff size={10} />{s.excluded}</span>}</span>
                  <span className="p" title={actionDisplay(c.action)}>{compactPath(actionDisplay(c.action), p.home)}</span>
                </div>
                <CategoryBadge category={c.category} lang={p.lang} />
                <RiskPill risk={c.risk} lang={p.lang} />
                <span className="age">{ageLabel(ageDays(c.modified), p.lang)}</span>
                <span className="size r">{c.size_exact ? "" : <span className="est">≈ </span>}{c.size ? humanSize(c.size) : "—"}</span>
                <div className="acts" onClick={(e) => e.stopPropagation()}>
                  <button type="button" className="ib" title={s.description} onClick={() => p.onActivate(p.activeId === c.id ? null : c.id)}><Info size={14} /></button>
                  {path && <button type="button" className="ib" title={s.reveal} onClick={() => p.onReveal(path)}><FolderOpen size={14} /></button>}
                  <button type="button" className="ib" title={isEx ? s.restore : s.exclude} onClick={() => p.onExclude(c.id)}>{isEx ? <RotateCcw size={14} /> : <EyeOff size={14} />}</button>
                </div>
              </div>
            );
          })}
            </div>
          ))}
          {visible.length > 0 && (
            <div className="tfoot">
              <ListFilter size={13} /><span><span className="num">{visible.length}</span> {s.items} · <span className="num">{humanSize(visibleTotal)}</span></span>
              <span style={{ marginLeft: 12, color: "var(--ink-3)" }}>{s.keyboardNav}</span>
              <span className="spacer" />
              <SquareCheckBig size={13} /><span><span className="num">{selectedVisible.length}</span> {s.selected.toLowerCase()} · <span className="num">{humanSize(selectedVisible.reduce((a, c) => a + c.size, 0))}</span></span>
            </div>
          )}
        </div>
        <ContextMenu menu={menu} onClose={() => setMenu(null)} />
        {active && <Detail c={active} lang={p.lang} home={p.home} excluded={p.excluded.has(active.id)} onClose={() => p.onActivate(null)} onExclude={() => p.onExclude(active.id)} onReveal={p.onReveal} onProtect={p.onProtect} />}
      </div>
    </>
  );
}

function Detail({ c, lang, home, excluded, onClose, onExclude, onReveal, onProtect }: { c: Candidate; lang: Lang; home: string; excluded: boolean; onClose: () => void; onExclude: () => void; onReveal: (p: string) => void; onProtect: (p: string) => void }) {
  const s = STRINGS[lang];
  const path = actionPath(c.action);
  const I = CATEGORY_ICON[c.category];
  return (
    <section className="panel detail">
      <div className="panel-b">
        <div style={{ display: "flex", alignItems: "flex-start", gap: 10 }}>
          <span className="cico" style={{ background: CATEGORY_VAR[c.category], flex: "none" }}><I size={15} /></span>
          <h3 style={{ flex: 1, paddingTop: 4 }}>{c.label}</h3>
          <button type="button" className="ib" onClick={onClose} aria-label={s.close}><X size={14} /></button>
        </div>
        <div className="chips"><CategoryBadge category={c.category} lang={lang} /><RiskPill risk={c.risk} lang={lang} /><Tags c={c} lang={lang} /></div>
        <p className="desc">{c.description}</p>
        <dl>
          <dt><FileText size={12} />{s.pathOrCommand}</dt><dd><code style={{ userSelect: "text" }}>{compactPath(actionDisplay(c.action), home)}</code></dd>
          <dt><ListFilter size={12} />{s.colSize}</dt><dd className="size">{c.size ? humanSize(c.size) : "—"}{!c.size_exact && <span className="tag mute" style={{ marginLeft: 6 }}>≈ {s.estimate}</span>}</dd>
          {c.modified && <><dt><Calendar size={12} />{s.modified}</dt><dd>{formatDate(c.modified, lang)} · {ageLabel(ageDays(c.modified), lang)}</dd></>}
          {c.project && <><dt><FolderGit2 size={12} />{s.project}</dt><dd>{c.project.name} <span className="tag mute">{c.project.kind}</span><br /><code>{compactPath(c.project.root, home)}</code><br /><span style={{ color: "var(--ink-3)" }}>{s.idle} {c.project.idle_days} {s.days}</span></dd></>}
          {c.owner && <><dt><Layers size={12} />{s.groupOwner}</dt><dd>{c.owner}</dd></>}
          {c.volume && <><dt><Usb size={12} />{s.groupVolume}</dt><dd><code>{c.volume}</code></dd></>}
          {c.blocked_by && <><dt><Lock size={12} />{s.blockedBy}</dt><dd className="blocked">{c.blocked_by}</dd></>}
          <dt><Fingerprint size={12} />ID</dt><dd><code style={{ userSelect: "text" }}>{c.id}</code></dd>
        </dl>
        <div className="acts2">
          {path && <button type="button" className="btn sm" onClick={() => onReveal(path)}><FolderOpen size={13} /> {s.reveal}</button>}
          <button type="button" className="btn sm" onClick={onExclude}>{excluded ? <RotateCcw size={13} /> : <EyeOff size={13} />} {excluded ? s.restore : s.exclude}</button>
          {c.action.type === "remove-path" && <button type="button" className="btn sm" title={s.protectHint} onClick={() => onProtect(c.action.type === "remove-path" ? c.action.path : "")}><ShieldPlus size={13} /> {s.protect}</button>}
          <button type="button" className="btn sm quiet" onClick={onClose}><FolderSearch size={13} /> {s.close}</button>
        </div>
      </div>
    </section>
  );
}
