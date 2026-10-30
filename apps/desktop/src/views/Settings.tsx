import { useEffect, useState } from "react";
import { FolderSearch, GitBranch, Globe, Recycle, RotateCcw, Save, Settings2 } from "lucide-react";
import type { Config } from "../types";
import type { Lang } from "../i18n";
import { STRINGS } from "../i18n";
import { Switch } from "../components/common";

const DEFAULTS: Config = {
  scan: { roots: [], default_roots: true, max_depth: 6, min_size_mb: 1, stale_days: 30, protected: [], disabled_rules: [] },
  git: { enabled: true, merged_branches: false, gc: true, worktree_idle_days: 7 },
  clean: { use_trash: true },
  ui: { language: "en" },
};

export function Settings({ lang, config, configPath, onSave }: { lang: Lang; config: Config; configPath: string; onSave: (c: Config) => Promise<void> }) {
  const s = STRINGS[lang];
  const [draft, setDraft] = useState<Config>(config);
  const [saving, setSaving] = useState(false);
  useEffect(() => setDraft(config), [config]);
  const dirty = JSON.stringify(draft) !== JSON.stringify(config);
  const set = <K extends keyof Config>(k: K, v: Partial<Config[K]>) => setDraft((d) => ({ ...d, [k]: { ...d[k], ...v } }));
  const lines = (v: string) => v.split("\n").map((x) => x.trim()).filter(Boolean);
  const num = (v: string, min: number) => Math.max(min, Number.parseInt(v || "0", 10) || 0);

  return (
    <form className="form" onSubmit={async (e) => { e.preventDefault(); setSaving(true); try { await onSave(draft); } finally { setSaving(false); } }}>
      <div className="sec"><Settings2 size={13} />{s.general}</div>
      <div className="panel field">
        <div className="swrow"><Switch id="set-trash" on={draft.clean.use_trash} onChange={(v) => set("clean", { use_trash: v })} /><label htmlFor="set-trash"><Recycle size={13} style={{ verticalAlign: -2, marginRight: 6, color: "var(--ember)" }} />{s.trashSetting}</label></div>
        <div className="help">{s.trashHint}</div>
      </div>
      <div className="panel field">
        <label htmlFor="set-lang"><Globe size={13} style={{ verticalAlign: -2, marginRight: 6, color: "var(--ember)" }} />{s.language}</label>
        <select id="set-lang" value={draft.ui.language} onChange={(e) => set("ui", { language: e.target.value as "en" | "tr" })}>
          <option value="en">English</option>
          <option value="tr">Türkçe</option>
        </select>
      </div>

      <div className="sec"><FolderSearch size={13} />{s.scanSection}</div>
      <div className="panel field">
        <div className="swrow"><Switch id="set-defroots" on={draft.scan.default_roots} onChange={(v) => set("scan", { default_roots: v })} /><label htmlFor="set-defroots">{s.defaultRoots}</label></div>
      </div>
      <div className="panel field">
        <label htmlFor="set-roots">{s.extraRoots}</label>
        <textarea id="set-roots" defaultValue={draft.scan.roots.join("\n")} key={`roots-${config.scan.roots.join()}`} onBlur={(e) => set("scan", { roots: lines(e.target.value) })} placeholder="~/code&#10;/Volumes/Work/projects" />
      </div>
      <div className="panel field f3">
        <div><label htmlFor="set-depth">{s.maxDepth}</label><input id="set-depth" type="number" min={1} max={12} value={draft.scan.max_depth} onChange={(e) => set("scan", { max_depth: num(e.target.value, 1) })} /></div>
        <div><label htmlFor="set-min">{s.minSize}</label><input id="set-min" type="number" min={0} value={draft.scan.min_size_mb} onChange={(e) => set("scan", { min_size_mb: num(e.target.value, 0) })} /></div>
        <div><label htmlFor="set-stale">{s.staleDays}</label><input id="set-stale" type="number" min={1} value={draft.scan.stale_days} onChange={(e) => set("scan", { stale_days: num(e.target.value, 1) })} /></div>
      </div>
      <div className="panel field">
        <label htmlFor="set-prot">{s.protectedExtra}</label>
        <textarea id="set-prot" defaultValue={draft.scan.protected.join("\n")} key={`prot-${config.scan.protected.join()}`} onBlur={(e) => set("scan", { protected: lines(e.target.value) })} placeholder="~/Important" />
      </div>
      <div className="panel field">
        <label htmlFor="set-rules">{s.disabledRules}</label>
        <textarea id="set-rules" defaultValue={draft.scan.disabled_rules.join("\n")} key={`rules-${config.scan.disabled_rules.join()}`} onBlur={(e) => set("scan", { disabled_rules: lines(e.target.value) })} placeholder="Ollama&#10;Android SDK" />
      </div>

      <div className="sec"><GitBranch size={13} />{s.gitSection}</div>
      <div className="panel field" style={{ gap: 12 }}>
        <div className="swrow"><Switch id="set-git" on={draft.git.enabled} onChange={(v) => set("git", { enabled: v })} /><label htmlFor="set-git">{s.gitEnabled}</label></div>
        <div className="swrow"><Switch id="set-branches" on={draft.git.merged_branches} onChange={(v) => set("git", { merged_branches: v })} /><label htmlFor="set-branches">{s.gitBranches}</label></div>
        <div className="swrow"><Switch id="set-gc" on={draft.git.gc} onChange={(v) => set("git", { gc: v })} /><label htmlFor="set-gc">{s.gitGc}</label></div>
        <div><label htmlFor="set-idle">{s.gitIdle}</label><input id="set-idle" type="number" min={0} value={draft.git.worktree_idle_days} onChange={(e) => set("git", { worktree_idle_days: num(e.target.value, 0) })} style={{ maxWidth: 140 }} /></div>
      </div>

      <div className="form-actions">
        <button type="submit" className="btn ember" disabled={!dirty || saving}><Save size={14} /> {s.save}</button>
        <button type="button" className="btn quiet" onClick={() => setDraft(DEFAULTS)}><RotateCcw size={14} /> {s.reset}</button>
        <span className="help" style={{ marginLeft: "auto", color: "var(--ink-3)", fontSize: 12 }}>{s.configAt}: <code style={{ userSelect: "text" }}>{configPath}</code></span>
      </div>
    </form>
  );
}
