import { useEffect, useMemo, useRef, useState } from "react";
import { Command, CornerDownLeft, Search } from "lucide-react";
import type { Candidate } from "../types";
import { actionDisplay } from "../types";
import { compactPath, humanSize } from "../format";
import type { Lang } from "../i18n";
import { STRINGS } from "../i18n";
import { CATEGORY_ICON, CATEGORY_VAR } from "./common";

export interface PaletteAction {
  id: string;
  label: string;
  hint?: string;
  icon: JSX.Element;
  run: () => void;
}

interface Props {
  lang: Lang;
  home: string;
  actions: PaletteAction[];
  candidates: Candidate[];
  onPick: (c: Candidate) => void;
  onClose: () => void;
}

export function CommandPalette({ lang, home, actions, candidates, onPick, onClose }: Props) {
  const s = STRINGS[lang];
  const [q, setQ] = useState("");
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => inputRef.current?.focus(), []);

  const items = useMemo(() => {
    const needle = q.trim().toLowerCase();
    const acts = actions.filter((a) => !needle || `${a.label} ${a.hint ?? ""}`.toLowerCase().includes(needle)).map((a) => ({ kind: "action" as const, a }));
    const cands = needle
      ? candidates.filter((c) => `${c.label} ${actionDisplay(c.action)} ${c.owner ?? ""} ${c.category}`.toLowerCase().includes(needle)).slice(0, 12).map((c) => ({ kind: "cand" as const, c }))
      : [];
    return [...acts, ...cands];
  }, [q, actions, candidates]);

  useEffect(() => setCursor(0), [q]);

  const activate = (i: number) => {
    const it = items[i];
    if (!it) return;
    if (it.kind === "action") it.a.run();
    else onPick(it.c);
    onClose();
  };

  return (
    <div className="backdrop" style={{ alignItems: "start", paddingTop: "12vh" }} onClick={onClose}>
      <div className="modal palette" onClick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
        <label className="pal-input">
          <Search size={16} />
          <input ref={inputRef} id="palette-input" placeholder={s.paletteHint} value={q} onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") { e.preventDefault(); setCursor((c) => Math.min(items.length - 1, c + 1)); }
              if (e.key === "ArrowUp") { e.preventDefault(); setCursor((c) => Math.max(0, c - 1)); }
              if (e.key === "Enter") { e.preventDefault(); activate(cursor); }
              if (e.key === "Escape") onClose();
            }} />
          <span className="kbd">esc</span>
        </label>
        <div className="pal-list">
          {items.length === 0 && <div className="pal-empty">{s.paletteNoResults}</div>}
          {items.map((it, i) => {
            if (it.kind === "action") {
              return (
                <button type="button" key={`a-${it.a.id}`} className={`pal-row ${i === cursor ? "cur" : ""}`} onMouseEnter={() => setCursor(i)} onClick={() => activate(i)}>
                  <span className="pal-ico">{it.a.icon}</span>
                  <span className="pal-main"><strong>{it.a.label}</strong>{it.a.hint && <small>{it.a.hint}</small>}</span>
                  {i === cursor && <CornerDownLeft size={13} style={{ color: "var(--ink-3)" }} />}
                </button>
              );
            }
            const I = CATEGORY_ICON[it.c.category];
            return (
              <button type="button" key={`c-${it.c.id}`} className={`pal-row ${i === cursor ? "cur" : ""}`} onMouseEnter={() => setCursor(i)} onClick={() => activate(i)}>
                <span className="pal-ico" style={{ background: CATEGORY_VAR[it.c.category], color: "#fff" }}><I size={14} /></span>
                <span className="pal-main"><strong>{it.c.label}</strong><small>{compactPath(actionDisplay(it.c.action), home)}</small></span>
                <span className="num" style={{ fontSize: 12, fontWeight: 600 }}>{humanSize(it.c.size)}</span>
              </button>
            );
          })}
        </div>
        <div className="pal-foot"><Command size={12} /> K · ↑↓ · ↵</div>
      </div>
    </div>
  );
}
