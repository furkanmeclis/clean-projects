import { useState, type ReactNode } from "react";
import {
  AppWindow, Check, Container, FolderGit2, GitBranch, Globe, Hammer, Hourglass, Package, Smartphone, Sparkles,
  Trash2, TriangleAlert, ShieldCheck, ShieldAlert, type LucideIcon,
} from "lucide-react";
import type { Candidate, Category, Risk } from "../types";
import type { Lang } from "../i18n";
import { CATEGORY_LABELS, STRINGS } from "../i18n";

export const CATEGORY_ICON: Record<Category, LucideIcon> = {
  project: FolderGit2,
  "package-cache": Package,
  "build-cache": Hammer,
  ide: AppWindow,
  browser: Globe,
  container: Container,
  mobile: Smartphone,
  git: GitBranch,
  "app-data": Sparkles,
  temp: Trash2,
};

export const CATEGORY_VAR: Record<Category, string> = {
  project: "var(--cat-project)",
  "package-cache": "var(--cat-packages)",
  "build-cache": "var(--cat-build)",
  ide: "var(--cat-ide)",
  browser: "var(--cat-browser)",
  container: "var(--cat-container)",
  mobile: "var(--cat-mobile)",
  git: "var(--cat-git)",
  "app-data": "var(--cat-appdata)",
  temp: "var(--cat-temp)",
};

export function CategoryIcon({ category, size = 15, className }: { category: Category; size?: number; className?: string }) {
  const I = CATEGORY_ICON[category];
  return <I size={size} className={className} />;
}

export function CategoryBadge({ category, lang }: { category: Category; lang: Lang }) {
  const I = CATEGORY_ICON[category];
  return (
    <span className="badge">
      <I size={12} style={{ color: CATEGORY_VAR[category] }} />
      {CATEGORY_LABELS[lang][category]}
    </span>
  );
}

export function RiskPill({ risk, lang }: { risk: Risk; lang: Lang }) {
  const s = STRINGS[lang];
  const label = risk === "low" ? s.low : risk === "medium" ? s.medium : s.high;
  const I = risk === "low" ? ShieldCheck : risk === "medium" ? ShieldAlert : TriangleAlert;
  return <span className={`pill ${risk}`}><I size={11} />{label}</span>;
}

export function CheckBox({ checked }: { checked: boolean }) {
  return <span className="cb">{checked && <Check size={14} strokeWidth={3} />}</span>;
}

export function Tags({ c, lang }: { c: Candidate; lang: Lang }) {
  const s = STRINGS[lang];
  const tags = c.tags ?? [];
  return (
    <>
      {tags.includes("stale") && <span className="tag"><Hourglass size={10} />{lang === "tr" ? "atıl" : "stale"}</span>}
      {tags.includes("merged") && <span className="tag mute">{s.merged}</span>}
      {!c.size_exact && <span className="tag mute">≈ {s.estimate}</span>}
    </>
  );
}

export function Switch({ on, onChange, id }: { on: boolean; onChange: (v: boolean) => void; id?: string }) {
  return <button type="button" id={id} role="switch" aria-checked={on} className={`sw ${on ? "on" : ""}`} onClick={() => onChange(!on)} />;
}

export function Panel({ icon, title, sub, children, className }: { icon: ReactNode; title: string; sub?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={`panel ${className ?? ""}`}>
      <div className="panel-h"><span className="ico">{icon}</span>{title}{sub && <span className="sub">{sub}</span>}</div>
      <div className="panel-b">{children}</div>
    </section>
  );
}

export function useTooltip() {
  const [tip, setTip] = useState<{ x: number; y: number; node: ReactNode } | null>(null);
  const show = (e: React.MouseEvent, node: ReactNode) => setTip({ x: e.clientX, y: e.clientY, node });
  const move = (e: React.MouseEvent) => setTip((t) => (t ? { ...t, x: e.clientX, y: e.clientY } : t));
  const hide = () => setTip(null);
  const el = tip ? <div className="tip" style={{ left: tip.x, top: tip.y }}>{tip.node}</div> : null;
  return { show, move, hide, el };
}
