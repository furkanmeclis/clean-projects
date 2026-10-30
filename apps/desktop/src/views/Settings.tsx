import { useEffect, useState } from "react";
import { Bell, FolderSearch, GitBranch, Globe, HardDrive, Monitor, Moon, Recycle, RotateCcw, Save, Settings2, ShieldAlert, Sun, SunMoon, Trash2 } from "lucide-react";
import type { Config } from "../types";
import type { Lang } from "../i18n";
import { STRINGS } from "../i18n";
import { Switch } from "../components/common";

const DEFAULTS: Config = {
  scan: { roots: [], default_roots: true, max_depth: 6, min_size_mb: 1, stale_days: 30, protected: [], disabled_rules: [], volumes: true, skip_volumes: [], installer_days: 7 },
  git: { enabled: true, merged_branches: false, gc: true, worktree_idle_days: 7 },
  clean: { use_trash: true, mode: "recovery", recovery_days: 14, block_running_apps: true },
  ui: { language: "en", close_to_tray: true, scan_interval_hours: 24, notify_threshold_gb: 5, check_updates: true, theme: "system" },
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
        <label>{s.modeLabel}</label>
        <div className="modes">
          <button type="button" className={draft.clean.mode === "recovery" ? "active" : ""} onClick={() => set("clean", { mode: "recovery", use_trash: true })}><Recycle size={15} /><span>{s.recoveryMode}</span><small>{s.recoveryHint}</small></button>
          <button type="button" className={draft.clean.mode === "trash" ? "active" : ""} onClick={() => set("clean", { mode: "trash", use_trash: true })}><Trash2 size={15} /><span>{s.trashMode}</span><small>{s.trashHint2}</small></button>
          <button type="button" className={draft.clean.mode === "permanent" ? "active" : ""} onClick={() => set("clean", { mode: "permanent", use_trash: false })}><ShieldAlert size={15} /><span>{s.permanentMode}</span><small>{s.permanentHint}</small></button>
        </div>
        <div className="help">{s.modeHint}</div>
        <div className="f3" style={{ marginTop: 6 }}>
          <div><label htmlFor="set-recdays">{s.recoveryDays}</label><input id="set-recdays" type="number" min={1} value={draft.clean.recovery_days} onChange={(e) => set("clean", { recovery_days: num(e.target.value, 1) })} /></div>
        </div>
        <div className="swrow"><Switch id="set-block" on={draft.clean.block_running_apps} onChange={(v) => set("clean", { block_running_apps: v })} /><label htmlFor="set-block">{s.blockRunning}</label></div>
      </div>
      <div className="panel field">
        <label>{s.theme}</label>
        <div className="seg" style={{ width: "fit-content" }}>
          {(["system", "light", "dark"] as const).map((t) => (
            <button type="button" key={t} className={draft.ui.theme === t ? "active" : ""} onClick={() => set("ui", { theme: t })}>
              {t === "system" ? <SunMoon size={13} /> : t === "light" ? <Sun size={13} /> : <Moon size={13} />} {t === "system" ? s.themeSystem : t === "light" ? s.themeLight : s.themeDark}
            </button>
          ))}
        </div>
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
      <div className="sec"><HardDrive size={13} />{s.volumes}</div>
      <div className="panel field" style={{ gap: 10 }}>
        <div className="swrow"><Switch id="set-vols" on={draft.scan.volumes} onChange={(v) => set("scan", { volumes: v })} /><label htmlFor="set-vols">{s.volumesSetting}</label></div>
        <label htmlFor="set-skipvols">{s.skipVolumes}</label>
        <textarea id="set-skipvols" defaultValue={draft.scan.skip_volumes.join("\n")} key={`skip-${config.scan.skip_volumes.join()}`} onBlur={(e) => set("scan", { skip_volumes: lines(e.target.value) })} placeholder="/Volumes/TimeMachine" />
        <div><label htmlFor="set-inst">{s.installerDays}</label><input id="set-inst" type="number" min={0} value={draft.scan.installer_days} onChange={(e) => set("scan", { installer_days: num(e.target.value, 0) })} style={{ maxWidth: 140 }} /></div>
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

      <div className="sec"><Monitor size={13} />{s.desktopSection}</div>
      <div className="panel field" style={{ gap: 12 }}>
        <div className="swrow"><Switch id="set-tray" on={draft.ui.close_to_tray} onChange={(v) => set("ui", { close_to_tray: v })} /><label htmlFor="set-tray">{s.closeToTray}</label></div>
        <div className="f3">
          <div><label htmlFor="set-interval"><Bell size={12} style={{ verticalAlign: -1, marginRight: 4 }} />{s.scanInterval}</label><input id="set-interval" type="number" min={0} value={draft.ui.scan_interval_hours} onChange={(e) => set("ui", { scan_interval_hours: num(e.target.value, 0) })} /></div>
          <div><label htmlFor="set-notify">{s.notifyThreshold}</label><input id="set-notify" type="number" min={0} value={draft.ui.notify_threshold_gb} onChange={(e) => set("ui", { notify_threshold_gb: num(e.target.value, 0) })} /></div>
        </div>
        <div className="swrow"><Switch id="set-upd" on={draft.ui.check_updates} onChange={(v) => set("ui", { check_updates: v })} /><label htmlFor="set-upd">{s.checkUpdates}</label></div>
      </div>

      <div className="form-actions">
        <button type="submit" className="btn ember" disabled={!dirty || saving}><Save size={14} /> {s.save}</button>
        <button type="button" className="btn quiet" onClick={() => setDraft(DEFAULTS)}><RotateCcw size={14} /> {s.reset}</button>
        <span className="help" style={{ marginLeft: "auto", color: "var(--ink-3)", fontSize: 12 }}>{s.configAt}: <code style={{ userSelect: "text" }}>{configPath}</code></span>
      </div>
    </form>
  );
}
