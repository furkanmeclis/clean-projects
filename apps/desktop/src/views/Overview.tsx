import { Boxes, Flame, FolderSearch, HardDrive, Hourglass, Layers, Lock, PieChart, Sparkles, Wand2, Wrench, Zap } from "lucide-react";
import type { Bootstrap, Candidate, Category, DiskInfo } from "../types";
import { CATEGORIES, actionDisplay } from "../types";
import { compactPath, humanSize, pct } from "../format";
import type { Lang } from "../i18n";
import { CATEGORY_LABELS, STRINGS } from "../i18n";
import { CATEGORY_ICON, CATEGORY_VAR, Panel, useTooltip } from "../components/common";

interface Props {
  lang: Lang;
  boot: Bootstrap;
  disk: DiskInfo;
  candidates: Candidate[];
  excluded: Set<string>;
  selected: Set<string>;
  onPick: (c: Candidate) => void;
  onPreset: (ids: string[]) => void;
}

function Ring({ disk, recoverable, selected }: { disk: DiskInfo; recoverable: number; selected: number }) {
  const r = 70;
  const C = 2 * Math.PI * r;
  const usedPct = disk.total ? disk.used / disk.total : 0;
  const recPct = disk.total ? recoverable / disk.total : 0;
  const selPct = disk.total ? selected / disk.total : 0;
  const seg = (from: number, len: number, color: string, w = 14) => (
    <circle cx="84" cy="84" r={r} fill="none" stroke={color} strokeWidth={w} strokeLinecap="butt"
      strokeDasharray={`${Math.max(0, len * C - 2)} ${C}`} strokeDashoffset={-from * C} transform="rotate(-90 84 84)" />
  );
  // Used space minus the recoverable part is drawn first; recoverable sits at the end of used space.
  const solid = Math.max(0, usedPct - recPct);
  return (
    <div className="ring">
      <svg viewBox="0 0 168 168" width="168" height="168" role="img" aria-label="Disk usage">
        <circle cx="84" cy="84" r={r} fill="none" stroke="var(--surface-2)" strokeWidth="14" />
        {seg(0, solid, "var(--line-2)")}
        {seg(solid, recPct, "var(--ember)")}
        {selPct > 0 && seg(solid, selPct, "var(--ember-2)", 20)}
      </svg>
      <div className="center">
        <div className="big">{humanSize(disk.free)}</div>
        <div className="lbl">free</div>
      </div>
    </div>
  );
}

export function Overview({ lang, boot, disk, candidates, excluded, selected, onPick, onPreset }: Props) {
  const s = STRINGS[lang];
  const tip = useTooltip();
  const active = candidates.filter((c) => !excluded.has(c.id));
  const recoverable = active.reduce((a, c) => a + c.size, 0);
  const selectedSize = candidates.filter((c) => selected.has(c.id)).reduce((a, c) => a + c.size, 0);
  const staleProjects = new Set(active.filter((c) => c.tags?.includes("stale")).map((c) => c.project?.root ?? c.id)).size;
  const staleSize = active.filter((c) => c.tags?.includes("stale")).reduce((a, c) => a + c.size, 0);
  const byCat = CATEGORIES.map((cat) => ({ cat, items: active.filter((c) => c.category === cat) }))
    .map((x) => ({ ...x, size: x.items.reduce((a, c) => a + c.size, 0) }))
    .filter((x) => x.size > 0)
    .sort((a, b) => b.size - a.size);
  const max = byCat[0]?.size ?? 1;
  const largest = [...active].sort((a, b) => b.size - a.size).slice(0, 7);
  const low = active.filter((c) => c.risk === "low");
  const presets = [
    { icon: <Zap size={16} />, title: lang === "tr" ? "Güvenli süpürme" : "Safe sweep", desc: lang === "tr" ? "Tüm düşük riskli öğeler" : "Every low-risk item", items: low },
    { icon: <Hourglass size={16} />, title: lang === "tr" ? "Atıl projeler" : "Stale projects", desc: lang === "tr" ? `${boot.config.scan.stale_days}+ gündür dokunulmamış` : `Untouched for ${boot.config.scan.stale_days}+ days`, items: active.filter((c) => c.tags?.includes("stale") && c.risk !== "high") },
    { icon: <Layers size={16} />, title: lang === "tr" ? "Derleme önbellekleri" : "Build caches", desc: "DerivedData, go-build, target, .next…", items: active.filter((c) => (c.category === "build-cache" || c.category === "project") && c.risk === "low") },
  ];

  return (
    <>
      <div className="ov-grid">
        <section className="panel hero">
          <Ring disk={disk} recoverable={recoverable} selected={selectedSize} />
          <div className="hero-stats">
            <div className="stat"><i style={{ background: "var(--line-2)" }} /><span className="k">{s.diskUsed}</span><span className="v">{humanSize(disk.used)} · {pct(disk.used, disk.total).toFixed(0)}%</span></div>
            <div className="stat"><i style={{ background: "var(--ember)" }} /><span className="k">{s.recoverable}</span><span className="v">{humanSize(recoverable)}</span></div>
            <div className="stat"><i style={{ background: "var(--ember-2)", outline: "2px solid var(--ember-soft)" }} /><span className="k">{s.selected}</span><span className="v">{humanSize(selectedSize)}</span></div>
            <div className="stat"><i style={{ background: "var(--surface-2)", border: "1px solid var(--line-2)" }} /><span className="k">{s.freeSpace}</span><span className="v">{humanSize(disk.free)} / {humanSize(disk.total)}</span></div>
          </div>
          <div className="hero-foot">
            <span className="chip"><HardDrive size={12} />{disk.mount || "/"}</span>
            <span className="chip"><Boxes size={12} />{active.length} {s.items}</span>
            {excluded.size > 0 && <span className="chip">{excluded.size} {s.excluded}</span>}
          </div>
        </section>
        <div className="ov-right">
        <div className="tile-row">
          <section className="panel tile ember"><span className="k"><Flame size={13} />{s.recoverable}</span><span className="v">{humanSize(recoverable)}</span><span className="h">{pct(recoverable, disk.total).toFixed(1)}% {s.of} {humanSize(disk.total)}</span></section>
          <section className="panel tile"><span className="k"><Sparkles size={13} />{s.low}</span><span className="v">{humanSize(low.reduce((a, c) => a + c.size, 0))}</span><span className="h">{low.length} {s.items}</span></section>
          <section className="panel tile"><span className="k"><Hourglass size={13} />{s.stale}</span><span className="v">{staleProjects}</span><span className="h">{humanSize(staleSize)} · ≥ {boot.config.scan.stale_days} {s.days}</span></section>
          <section className="panel tile"><span className="k"><PieChart size={13} />{s.candidates}</span><span className="v">{active.length}</span><span className="h">{byCat.length} {lang === "tr" ? "kategori" : "categories"}</span></section>
        </div>
          <Panel icon={<Wand2 size={14} />} title={lang === "tr" ? "Hızlı seçimler" : "Quick selections"} className="presets-panel" sub={<span>{lang === "tr" ? "seçime ekler" : "adds to selection"}</span>}>
            <div className="presets">
              {presets.map((p) => (
                <button type="button" key={p.title} className="preset" disabled={p.items.length === 0} onClick={() => onPreset(p.items.map((c) => c.id))}>
                  <span className="pi">{p.icon}</span>
                  <span><span className="pt">{p.title}</span><br /><span className="pd">{p.desc}</span></span>
                  <span className="ps">{humanSize(p.items.reduce((a, c) => a + c.size, 0))}<br /><span style={{ color: "var(--ink-3)", fontWeight: 500 }}>{p.items.length} {s.items}</span></span>
                </button>
              ))}
            </div>
          </Panel>
        </div>
      </div>

      <div className="ov-mid">
        <Panel icon={<PieChart size={14} />} title={s.byCategory} sub={<span className="num">{humanSize(recoverable)}</span>}>
          {byCat.length === 0 ? (
            <div className="empty"><span className="ei"><FolderSearch size={24} /></span><strong>{candidates.length ? s.tidy : s.emptyScan}</strong></div>
          ) : (
            <div className="chart" onMouseLeave={tip.hide}>
              {byCat.map(({ cat, size, items }) => {
                const I = CATEGORY_ICON[cat as Category];
                return (
                  <div className="crow" key={cat}
                    onMouseEnter={(e) => tip.show(e, <span>{CATEGORY_LABELS[lang][cat]} · <b>{humanSize(size)}</b> · {items.length} {s.items} · {pct(size, recoverable).toFixed(0)}%</span>)}
                    onMouseMove={tip.move}>
                    <span className="ci"><I size={14} /></span>
                    <span className="cn">{CATEGORY_LABELS[lang][cat]}</span>
                    <span className="ct"><i style={{ width: `${pct(size, max)}%`, background: CATEGORY_VAR[cat as Category] }} /></span>
                    <span className="cv">{humanSize(size)}</span>
                  </div>
                );
              })}
            </div>
          )}
        </Panel>
        <Panel icon={<Flame size={14} />} title={s.largest}>
          <div className="biglist">
            {largest.map((c) => {
              const I = CATEGORY_ICON[c.category];
              return (
                <button type="button" className="bigrow" key={c.id} onClick={() => onPick(c)}>
                  <span className="bi" style={{ background: CATEGORY_VAR[c.category] }}><I size={15} /></span>
                  <span style={{ minWidth: 0 }}><span className="bn">{c.label}</span><span className="bp">{compactPath(actionDisplay(c.action), boot.home)}</span></span>
                  <span className="bs">{humanSize(c.size)}</span>
                </button>
              );
            })}
            {largest.length === 0 && <div className="empty"><span>{s.emptyScan}</span></div>}
          </div>
        </Panel>
      </div>

      <div className="ov-low">
        <Panel icon={<Lock size={14} />} title={s.protectedTitle} sub={s.protectedHint}>
          <div className="chips">{boot.protected.map((p) => <span className="chip" key={p}><Lock size={11} />{p}</span>)}</div>
        </Panel>
        <Panel icon={<FolderSearch size={14} />} title={s.roots}>
          <div className="chips">{boot.roots.map((p) => <span className="chip" key={p}><FolderSearch size={11} />{p}</span>)}{boot.roots.length === 0 && <span className="chip off">—</span>}</div>
        </Panel>
        <Panel icon={<Wrench size={14} />} title={s.tools}>
          <div className="chips">{boot.tools.map(([t, ok]) => <span className={`chip ${ok ? "on" : "off"}`} key={t}>{ok ? "●" : "○"} {t}</span>)}</div>
        </Panel>
      </div>
      {tip.el}
    </>
  );
}
