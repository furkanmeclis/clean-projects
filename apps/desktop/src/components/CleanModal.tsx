import { useEffect, useRef, useState } from "react";
import { Check, CircleCheckBig, FlaskConical, Loader2, Recycle, ShieldAlert, Trash2, TriangleAlert, X, Layers, HardDrive } from "lucide-react";
import type { Candidate, HistoryEntry } from "../types";
import { humanSize } from "../format";
import type { Lang } from "../i18n";
import { STRINGS } from "../i18n";

export type Phase =
  | { kind: "confirm" }
  | { kind: "running"; index: number; total: number; label: string; log: LogLine[] }
  | { kind: "done"; entry: HistoryEntry; log: LogLine[] };

export interface LogLine { text: string; tone?: "ok" | "fail" | "head"; }

interface Props {
  lang: Lang;
  items: Candidate[];
  phase: Phase;
  useTrash: boolean;
  dryRun: boolean;
  onUseTrash: (v: boolean) => void;
  onDryRun: (v: boolean) => void;
  onRun: () => void;
  onClose: () => void;
}

export function CleanModal({ lang, items, phase, useTrash, dryRun, onUseTrash, onDryRun, onRun, onClose }: Props) {
  const s = STRINGS[lang];
  const total = items.reduce((a, c) => a + c.size, 0);
  const high = items.filter((c) => c.risk === "high").length;
  const logRef = useRef<HTMLDivElement>(null);
  const [showLog, setShowLog] = useState(false);
  useEffect(() => { if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight; });
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && phase.kind !== "running") onClose();
      if (e.key === "Enter" && phase.kind === "confirm") onRun();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [phase.kind, onClose, onRun]);

  return (
    <div className="backdrop" onClick={() => phase.kind !== "running" && onClose()}>
      <div className="modal" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        {phase.kind === "confirm" && (
          <>
            <div className="m-h"><span className="mi"><Trash2 size={17} /></span><h2>{s.confirmTitle}</h2></div>
            <div className="m-b">
              <div className="sum3">
                <div><span><Layers size={11} />{s.items}</span><strong>{items.length}</strong></div>
                <div><span><HardDrive size={11} />{s.recoverable}</span><strong>{humanSize(total)}</strong></div>
                <div><span><ShieldAlert size={11} />{s.high}</span><strong style={{ color: high ? "var(--bad)" : undefined }}>{high}</strong></div>
              </div>
              <p>{s.confirmBody}</p>
              {high > 0 && <div className="warnbox"><TriangleAlert size={16} /> {s.highWarning}</div>}
              <div className="modes">
                <button type="button" className={useTrash && !dryRun ? "active" : ""} onClick={() => { onUseTrash(true); onDryRun(false); }}><Recycle size={15} /><span>{s.trashMode}</span><small>{lang === "tr" ? "geri alınabilir" : "recoverable"}</small></button>
                <button type="button" className={!useTrash && !dryRun ? "active" : ""} onClick={() => { onUseTrash(false); onDryRun(false); }}><Trash2 size={15} /><span>{s.permanentMode}</span><small>{lang === "tr" ? "geri alınamaz" : "irreversible"}</small></button>
                <button type="button" className={dryRun ? "active" : ""} onClick={() => onDryRun(true)}><FlaskConical size={15} /><span>{s.dryRun}</span><small>{lang === "tr" ? "sadece göster" : "simulate only"}</small></button>
              </div>
            </div>
            <div className="m-f">
              <span style={{ color: "var(--ink-3)", fontSize: 12, marginRight: "auto", display: "flex", gap: 6, alignItems: "center" }}><span className="kbd">Esc</span>{s.cancel}<span className="kbd" style={{ marginLeft: 8 }}>↵</span>{s.run}</span>
              <button type="button" className="btn quiet" onClick={onClose}>{s.cancel}</button>
              <button type="button" className={`btn ${dryRun ? "ink" : useTrash ? "ember" : "danger"}`} onClick={onRun}>{dryRun ? <FlaskConical size={14} /> : useTrash ? <Recycle size={14} /> : <Trash2 size={14} />}{dryRun ? s.dryRun : s.run}</button>
            </div>
          </>
        )}
        {phase.kind === "running" && (
          <>
            <div className="m-h"><span className="mi"><Loader2 size={17} className="spin" /></span><h2>{s.running}</h2></div>
            <div className="m-b">
              <div className="progress">
                <div className="lbl"><span>{phase.label}</span><span className="num">{phase.index + 1} / {phase.total}</span></div>
                <div className="meter"><i style={{ width: `${((phase.index + 1) / phase.total) * 100}%` }} /></div>
              </div>
              <div className="log" ref={logRef}>{phase.log.map((l, i) => <div key={i} className={l.tone}>{l.text}</div>)}</div>
            </div>
          </>
        )}
        {phase.kind === "done" && (
          <>
            <div className="m-h"><span className="mi good"><CircleCheckBig size={17} /></span><h2>{s.done}{phase.entry.dry_run ? ` · ${s.dryRun}` : ""}</h2></div>
            <div className="m-b">
              <div className="res">
                <div className="big"><strong>{humanSize(phase.entry.freed)}</strong><span>{s.freed}</span></div>
                <div><Check size={16} style={{ color: "var(--good)" }} /><strong>{phase.entry.items.filter((i) => i.ok).length}</strong><span>{s.ok}</span></div>
                <div><X size={16} style={{ color: phase.entry.items.some((i) => !i.ok) ? "var(--bad)" : "var(--ink-3)" }} /><strong>{phase.entry.items.filter((i) => !i.ok).length}</strong><span>{s.failed}</span></div>
              </div>
              {phase.entry.items.filter((i) => !i.ok).map((i) => <div className="warnbox" key={i.id}><X size={14} /> {i.label}: {i.error}</div>)}
              {showLog && <div className="log" ref={logRef}>{phase.log.map((l, i) => <div key={i} className={l.tone}>{l.text}</div>)}</div>}
            </div>
            <div className="m-f">
              <button type="button" className="btn quiet" onClick={() => setShowLog(!showLog)}>{s.log}</button>
              <button type="button" className="btn ink" onClick={onClose}>{s.close}</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
