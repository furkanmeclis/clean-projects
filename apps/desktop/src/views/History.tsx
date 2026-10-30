import { useState } from "react";
import { Check, ChevronDown, ChevronRight, History as HistoryIcon, Recycle, Trash2, TrendingUp, X, FlaskConical } from "lucide-react";
import type { HistoryEntry } from "../types";
import { compactPath, formatDate, humanSize } from "../format";
import type { Lang } from "../i18n";
import { STRINGS } from "../i18n";
import { Panel } from "../components/common";

function Spark({ values }: { values: number[] }) {
  const w = 420, h = 110, pad = 8;
  if (values.length === 0) return null;
  const max = Math.max(...values, 1);
  const step = values.length > 1 ? (w - pad * 2) / (values.length - 1) : 0;
  const pts = values.map((v, i) => [pad + i * step, h - pad - (v / max) * (h - pad * 2)] as const);
  const d = pts.map(([x, y], i) => `${i ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
  const area = `${d} L${pts[pts.length - 1][0].toFixed(1)},${h - pad} L${pts[0][0].toFixed(1)},${h - pad} Z`;
  const last = pts[pts.length - 1];
  return (
    <svg className="spark" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" role="img" aria-label="Freed per cleanup">
      <defs><linearGradient id="sg" x1="0" x2="0" y1="0" y2="1"><stop offset="0" stopColor="var(--ember)" stopOpacity=".35" /><stop offset="1" stopColor="var(--ember)" stopOpacity="0" /></linearGradient></defs>
      {[0.25, 0.5, 0.75].map((f) => <line key={f} x1={pad} x2={w - pad} y1={h - pad - f * (h - pad * 2)} y2={h - pad - f * (h - pad * 2)} stroke="var(--line)" strokeWidth="1" />)}
      <path d={area} fill="url(#sg)" />
      <path d={d} fill="none" stroke="var(--ember)" strokeWidth="2" strokeLinejoin="round" strokeLinecap="round" />
      {values.length === 1 ? <circle cx={pts[0][0]} cy={pts[0][1]} r="4" fill="var(--ember)" /> : <circle cx={last[0]} cy={last[1]} r="4.5" fill="var(--ember)" stroke="var(--surface)" strokeWidth="2" />}
    </svg>
  );
}

export function History({ lang, home, history, onClear }: { lang: Lang; home: string; history: HistoryEntry[]; onClear: () => void }) {
  const s = STRINGS[lang];
  const [open, setOpen] = useState<number | null>(history.length ? history.length - 1 : null);
  const real = history.filter((h) => !h.dry_run);
  const total = real.reduce((a, h) => a + h.freed, 0);
  if (history.length === 0) return <div className="panel"><div className="empty"><span className="ei"><HistoryIcon size={24} /></span><strong>{s.historyEmpty}</strong></div></div>;
  return (
    <>
      <div className="hist-top">
        <section className="panel tile ember"><span className="k"><TrendingUp size={13} />{s.totalFreed}</span><span className="v">{humanSize(total)}</span><span className="h">{real.length} {lang === "tr" ? "temizlik" : "cleanups"} · {real.reduce((a, h) => a + h.items.filter((i) => i.ok).length, 0)} {s.items}</span></section>
        <Panel icon={<TrendingUp size={14} />} title={lang === "tr" ? "Çalıştırma başına kazanılan" : "Freed per run"} sub={<button type="button" className="btn sm quiet" onClick={onClear}><Trash2 size={13} /> {s.clearHistory}</button>}>
          <Spark values={real.map((h) => h.freed)} />
        </Panel>
      </div>
      {[...history].map((h, i) => ({ h, i })).reverse().map(({ h, i }) => {
        const ok = h.items.filter((x) => x.ok).length;
        const failed = h.items.length - ok;
        const isOpen = open === i;
        return (
          <section className="panel hist" key={i}>
            <div className="hist-h" onClick={() => setOpen(isOpen ? null : i)}>
              <span className="hi">{h.dry_run ? <FlaskConical size={14} /> : h.trashed ? <Recycle size={14} /> : <Trash2 size={14} />}</span>
              <div>
                <div className="w">{isOpen ? <ChevronDown size={13} /> : <ChevronRight size={13} />} {formatDate(h.at, lang)}</div>
                <div className="m"><span>{h.items.length} {s.items}</span><span style={{ color: "var(--good)" }}>{ok} {s.ok}</span>{failed > 0 && <span style={{ color: "var(--bad)" }}>{failed} {s.failed}</span>}{h.dry_run && <span>{s.dryRun}</span>}</div>
              </div>
              <span className="f">{humanSize(h.freed)}</span>
            </div>
            {isOpen && (
              <div className="hist-items">
                {h.items.map((it, j) => (
                  <div className="hi-row" key={j}>
                    {it.ok ? <Check size={14} className="ok" /> : <X size={14} className="fail" />}
                    <span className="size">{humanSize(it.size)}</span>
                    <span style={{ minWidth: 0 }}><strong>{it.label}</strong> <code title={it.detail}>{compactPath(it.detail, home)}</code>{it.error && <code style={{ color: "var(--bad)" }}>{it.error}</code>}</span>
                  </div>
                ))}
              </div>
            )}
          </section>
        );
      })}
    </>
  );
}
