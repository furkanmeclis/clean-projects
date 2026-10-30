import { useState } from "react";
import { CalendarClock, Check, Clock3, FolderSearch, Loader2, Pencil, Play, Plus, Sparkles, Trash2, X } from "lucide-react";
import type { Category, Risk, Routine, RoutineRun, Schedule } from "../types";
import { CATEGORIES } from "../types";
import { formatDate, humanSize } from "../format";
import type { Lang } from "../i18n";
import { CATEGORY_LABELS, STRINGS } from "../i18n";
import { CATEGORY_ICON, Panel, Switch } from "../components/common";

interface Props {
  lang: Lang;
  routines: Routine[];
  runs: RoutineRun[];
  running: boolean;
  onSave: (r: Routine) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onRun: (id: string) => Promise<void>;
  onTemplates: () => Promise<void>;
}

function describe(s: Schedule, lang: Lang): string {
  const t = STRINGS[lang];
  const days = t.weekdays.split(",");
  const hh = (h: number) => `${String(h).padStart(2, "0")}:00`;
  switch (s.kind) {
    case "daily": return `${t.routineDaily} · ${hh(s.hour)}`;
    case "weekly": return `${t.routineWeekly} · ${days[s.weekday] ?? ""} ${hh(s.hour)}`;
    case "monthly": return `${t.routineMonthly} · ${s.day}. ${hh(s.hour)}`;
    case "hours": return `${t.routineHours.replace("N", String(s.hours))}`;
  }
}

const EMPTY: Routine = { id: "", name: "", enabled: true, schedule: { kind: "weekly", weekday: 6, hour: 3 }, max_risk: "low", categories: [], stale_only: false, min_age_days: 0, mode: "recovery", roots: [], notify: true };

export function Routines({ lang, routines, runs, running, onSave, onDelete, onRun, onTemplates }: Props) {
  const s = STRINGS[lang];
  const [editing, setEditing] = useState<Routine | null>(null);
  const lastRun = (id: string) => [...runs].reverse().find((r) => r.routine_id === id);
  return (
    <>
      <div className="toolbar">
        <p className="help" style={{ color: "var(--ink-2)", maxWidth: 640 }}>{s.routinesHint}</p>
        <div className="right">
          <button type="button" className="btn" onClick={onTemplates}><Sparkles size={14} /> {s.routineTemplates}</button>
          <button type="button" className="btn ember" onClick={() => setEditing({ ...EMPTY })}><Plus size={14} /> {s.routineNew}</button>
        </div>
      </div>
      {routines.length === 0 && <div className="panel" style={{ marginBottom: 14 }}><div className="empty"><span className="ei"><CalendarClock size={24} /></span><strong>{s.routineNew}</strong><span>{s.routineSchedulerHint}</span></div></div>}
      <div className="rt-grid">
        {routines.map((r) => {
          const last = lastRun(r.id);
          return (
            <section className={`panel rt ${r.enabled ? "" : "off"}`} key={r.id}>
              <div className="rt-h">
                <Switch on={r.enabled} onChange={(v) => onSave({ ...r, enabled: v })} />
                <strong>{r.name}</strong>
                <span className="badge"><Clock3 size={11} />{describe(r.schedule, lang)}</span>
              </div>
              <div className="rt-meta">
                <span className={`pill ${r.max_risk}`}>≤ {r.max_risk}</span>
                {r.categories.length === 0 ? <span className="tag mute">{s.all}</span> : r.categories.map((c) => { const I = CATEGORY_ICON[c]; return <span className="badge" key={c}><I size={11} />{CATEGORY_LABELS[lang][c]}</span>; })}
                {r.stale_only && <span className="tag">{s.stale}</span>}
                {r.min_age_days > 0 && <span className="tag mute">≥ {r.min_age_days} {s.days}</span>}
                {r.roots.length > 0 && <span className="tag mute"><FolderSearch size={10} />{r.roots.length}</span>}
                <span className="tag mute">{r.mode}</span>
              </div>
              <div className="rt-last">
                {last ? <><Check size={12} style={{ color: last.failed ? "var(--warn)" : "var(--good)" }} />{s.routineLastRun} {formatDate(last.at, lang)} · <b className="num">{humanSize(last.freed)}</b> · {last.items} {s.items}{last.failed ? ` · ${last.failed} ${s.failed}` : ""}</> : <span>{s.routineNever}</span>}
              </div>
              <div className="rt-acts">
                <button type="button" className="btn sm" disabled={running} onClick={() => onRun(r.id)}>{running ? <Loader2 size={12} className="spin" /> : <Play size={12} />} {s.routineRunNow}</button>
                <button type="button" className="btn sm quiet" onClick={() => setEditing({ ...r })}><Pencil size={12} /> {s.routineEdit}</button>
                <button type="button" className="btn sm quiet" onClick={() => onDelete(r.id)}><Trash2 size={12} /> {s.routineDelete}</button>
              </div>
            </section>
          );
        })}
      </div>
      {runs.length > 0 && (
        <div style={{ marginTop: 14 }}>
          <Panel icon={<Clock3 size={14} />} title={s.routineRuns}>
            <div className="rt-runs">
              {[...runs].reverse().slice(0, 30).map((run, i) => (
                <div className="rt-run" key={i}>
                  <span><strong>{run.name}</strong>{run.error && <span className="tag" style={{ marginLeft: 6 }}>{run.error}</span>}</span>
                  <span className="when">{formatDate(run.at, lang)}</span>
                  <span className="num" style={{ fontWeight: 700, color: run.freed ? "var(--ember-2)" : "var(--ink-3)" }}>{humanSize(run.freed)} · {run.items}{run.failed ? ` / ${run.failed} ✗` : ""}</span>
                </div>
              ))}
            </div>
          </Panel>
        </div>
      )}
      <p className="help" style={{ color: "var(--ink-3)", fontSize: 12, marginTop: 12 }}>{s.routineSchedulerHint}</p>
      {editing && <Editor lang={lang} routine={editing} onClose={() => setEditing(null)} onSave={async (r) => { await onSave(r); setEditing(null); }} />}
    </>
  );
}

function Editor({ lang, routine, onClose, onSave }: { lang: Lang; routine: Routine; onClose: () => void; onSave: (r: Routine) => Promise<void> }) {
  const s = STRINGS[lang];
  const [r, setR] = useState<Routine>(routine);
  const days = s.weekdays.split(",");
  const sched = r.schedule;
  const setSched = (patch: Partial<Schedule> & { kind?: Schedule["kind"] }) => {
    const kind = patch.kind ?? sched.kind;
    const base: Schedule = kind === "daily" ? { kind, hour: 3 } : kind === "weekly" ? { kind, weekday: 6, hour: 3 } : kind === "monthly" ? { kind, day: 1, hour: 3 } : { kind, hours: 6 };
    setR({ ...r, schedule: { ...(sched.kind === kind ? sched : base), ...patch } as Schedule });
  };
  const toggleCat = (c: Category) => setR({ ...r, categories: r.categories.includes(c) ? r.categories.filter((x) => x !== c) : [...r.categories, c] });
  return (
    <div className="backdrop" onClick={onClose}>
      <div className="modal" style={{ width: "min(720px, calc(100vw - 48px))" }} onClick={(e) => e.stopPropagation()}>
        <div className="m-h"><span className="mi"><CalendarClock size={17} /></span><h2>{r.id ? s.routineEdit : s.routineNew}</h2><button type="button" className="ib" style={{ marginLeft: "auto" }} onClick={onClose}><X size={14} /></button></div>
        <div className="m-b" style={{ gap: 14, maxHeight: "70vh", overflow: "auto" }}>
          <div className="field" style={{ padding: 0 }}>
            <label htmlFor="rt-name">{s.routineName}</label>
            <input id="rt-name" type="text" value={r.name} onChange={(e) => setR({ ...r, name: e.target.value })} style={{ padding: "8px 10px", border: "1px solid var(--line-2)", borderRadius: 9, background: "var(--bg)", maxWidth: 420 }} />
          </div>
          <div className="field" style={{ padding: 0 }}>
            <label>{s.routineSchedule}</label>
            <div className="seg" style={{ width: "fit-content" }}>
              {(["daily", "weekly", "monthly", "hours"] as const).map((k) => (
                <button type="button" key={k} className={sched.kind === k ? "active" : ""} onClick={() => setSched({ kind: k })}>{k === "daily" ? s.routineDaily : k === "weekly" ? s.routineWeekly : k === "monthly" ? s.routineMonthly : s.routineHours.replace("N", "N")}</button>
              ))}
            </div>
            <div className="f3">
              {sched.kind === "weekly" && <div><label>{s.routineWeekday}</label><select value={sched.weekday} onChange={(e) => setSched({ weekday: Number(e.target.value) })}>{days.map((d, i) => <option key={d} value={i}>{d}</option>)}</select></div>}
              {sched.kind === "monthly" && <div><label>{s.routineDay}</label><input type="number" min={1} max={28} value={sched.day} onChange={(e) => setSched({ day: Math.min(28, Math.max(1, Number(e.target.value) || 1)) })} /></div>}
              {sched.kind !== "hours" && <div><label>{s.routineHour}</label><input type="number" min={0} max={23} value={sched.hour} onChange={(e) => setSched({ hour: Math.min(23, Math.max(0, Number(e.target.value) || 0)) })} /></div>}
              {sched.kind === "hours" && <div><label>{s.routineHours}</label><input type="number" min={1} value={sched.hours} onChange={(e) => setSched({ hours: Math.max(1, Number(e.target.value) || 1) })} /></div>}
            </div>
          </div>
          <div className="field" style={{ padding: 0 }}>
            <label>{s.routineMaxRisk}</label>
            <div className="seg" style={{ width: "fit-content" }}>
              {(["low", "medium"] as Risk[]).map((k) => <button type="button" key={k} className={`${k === "low" ? "lo" : "me"} ${r.max_risk === k ? "active" : ""}`} onClick={() => setR({ ...r, max_risk: k })}>{k === "low" ? s.low : s.medium}</button>)}
            </div>
          </div>
          <div className="field" style={{ padding: 0 }}>
            <label>{s.routineCategories}</label>
            <div className="chk-grid">
              {CATEGORIES.map((c) => { const I = CATEGORY_ICON[c]; const on = r.categories.includes(c); return <button type="button" key={c} className={`chk ${on ? "on" : ""}`} onClick={() => toggleCat(c)}><I size={13} />{CATEGORY_LABELS[lang][c]}</button>; })}
            </div>
          </div>
          <div className="f3">
            <div className="swrow"><Switch on={r.stale_only} onChange={(v) => setR({ ...r, stale_only: v })} /><label>{s.routineStale}</label></div>
            <div><label>{s.routineMinAge}</label><input type="number" min={0} value={r.min_age_days} onChange={(e) => setR({ ...r, min_age_days: Math.max(0, Number(e.target.value) || 0) })} /></div>
            <div><label>{s.modeLabel}</label><select value={r.mode} onChange={(e) => setR({ ...r, mode: e.target.value as "recovery" | "trash" })}><option value="recovery">{s.recoveryMode}</option><option value="trash">{s.trashMode}</option></select></div>
          </div>
          <div className="field" style={{ padding: 0 }}>
            <label>{s.routineRoots}</label>
            <textarea defaultValue={r.roots.join("\n")} onBlur={(e) => setR({ ...r, roots: e.target.value.split("\n").map((x) => x.trim()).filter(Boolean) })} placeholder="~/Projects" />
          </div>
          <div className="swrow"><Switch on={r.notify} onChange={(v) => setR({ ...r, notify: v })} /><label>{s.routineNotify}</label></div>
        </div>
        <div className="m-f">
          <button type="button" className="btn quiet" onClick={onClose}>{s.cancel}</button>
          <button type="button" className="btn ember" disabled={!r.name.trim()} onClick={() => onSave(r)}><Check size={14} /> {s.save}</button>
        </div>
      </div>
    </div>
  );
}
