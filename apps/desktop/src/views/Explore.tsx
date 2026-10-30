import { useEffect, useMemo, useState } from "react";
import { ArrowUp, ChevronRight, FileStack, FolderOpen, FolderPlus, HardDrive, Home, LayoutGrid, List, Loader2, ShieldPlus, Usb } from "lucide-react";
import type { Api } from "../api";
import type { Node, TreeNode, Volume } from "../types";
import { compactPath, humanSize } from "../format";
import type { Lang } from "../i18n";
import { STRINGS } from "../i18n";
import { useTooltip } from "../components/common";

interface Props {
  lang: Lang;
  api: Api;
  home: string;
  volumes: Volume[];
  onAdd: (path: string) => void;
  onProtect: (path: string) => void;
  onReveal: (path: string) => void;
}

interface Rect { x: number; y: number; w: number; h: number; }

// Squarified treemap (Bruls, Huizing, van Wijk).
function squarify(items: { size: number }[], rect: Rect): Rect[] {
  const total = items.reduce((a, i) => a + i.size, 0);
  if (total <= 0 || items.length === 0) return items.map(() => ({ x: rect.x, y: rect.y, w: 0, h: 0 }));
  const out: Rect[] = new Array(items.length);
  let { x, y, w, h } = rect;
  let i = 0;
  const scale = (w * h) / total;
  while (i < items.length) {
    const vertical = w >= h;
    const side = vertical ? h : w;
    let row: number[] = [];
    let rowSum = 0;
    let worst = Infinity;
    let j = i;
    while (j < items.length) {
      const area = items[j].size * scale;
      const nextSum = rowSum + area;
      const nextRow = [...row, area];
      const len = nextSum / side;
      const ratios = nextRow.map((a) => Math.max((a / len) / len, len / (a / len)));
      const nextWorst = Math.max(...ratios);
      if (row.length && nextWorst > worst) break;
      row = nextRow; rowSum = nextSum; worst = nextWorst; j++;
    }
    const len = rowSum / side;
    let off = 0;
    for (let k = 0; k < row.length; k++) {
      const seg = row[k] / len;
      out[i + k] = vertical ? { x, y: y + off, w: len, h: seg } : { x: x + off, y, w: seg, h: len };
      off += seg;
    }
    if (vertical) { x += len; w -= len; } else { y += len; h -= len; }
    i += row.length;
  }
  return out;
}

const PALETTE = ["var(--cat-project)", "var(--cat-ide)", "var(--cat-container)", "var(--cat-browser)", "var(--cat-packages)", "var(--cat-mobile)", "var(--cat-appdata)", "var(--cat-build)"];

export function Explore({ lang, api, home, volumes, onAdd, onProtect, onReveal }: Props) {
  const s = STRINGS[lang];
  const [dir, setDir] = useState(home);
  const [tree, setTree] = useState<TreeNode[]>([]);
  const [list, setList] = useState<Node[]>([]);
  const [loading, setLoading] = useState(false);
  const [view, setView] = useState<"map" | "list" | "large">("map");
  const [large, setLarge] = useState<Node[] | null>(null);
  const [largeLoading, setLargeLoading] = useState(false);
  useEffect(() => {
    if (view !== "large" || large !== null || largeLoading) return;
    setLargeLoading(true);
    api.largeFiles(null, 256, 80).then((l) => { setLarge(l); setLargeLoading(false); }).catch(() => setLargeLoading(false));
  }, [view, large, largeLoading, api]);
  const tip = useTooltip();

  useEffect(() => {
    let alive = true;
    setLoading(true);
    Promise.all([api.exploreTree(dir, 2, 24), api.exploreDir(dir)]).then(([t, l]) => {
      if (!alive) return;
      setTree(t); setList(l); setLoading(false);
    }).catch(() => setLoading(false));
    return () => { alive = false; };
  }, [dir, api]);

  const total = list.reduce((a, n) => a + n.size, 0);
  const W = 1000, H = 560;
  const layout = useMemo(() => {
    const items = tree.filter((t) => t.size > 0);
    const rects = squarify(items, { x: 0, y: 0, w: W, h: H });
    return items.map((t, i) => {
      const r = rects[i];
      const kids = t.children.filter((k) => k.size > 0);
      const pad = 3;
      const inner = { x: r.x + pad, y: r.y + 18, w: Math.max(0, r.w - pad * 2), h: Math.max(0, r.h - 18 - pad) };
      const kr = r.w > 60 && r.h > 40 ? squarify(kids, inner) : [];
      return { t, r, kids: kids.map((k, j) => ({ k, r: kr[j] })), color: PALETTE[i % PALETTE.length] };
    });
  }, [tree]);

  const crumbs = useMemo(() => {
    const vol = [...volumes].sort((a, b) => b.mount.length - a.mount.length).find((v) => dir.startsWith(v.mount));
    const base = vol?.mount ?? "/";
    const rest = dir.slice(base.length).split("/").filter(Boolean);
    const out: { name: string; path: string }[] = [{ name: vol?.name ?? "/", path: base }];
    let acc = base.replace(/\/$/, "");
    for (const part of rest) { acc += "/" + part; out.push({ name: part, path: acc }); }
    return out;
  }, [dir, volumes]);

  const up = () => { const p = dir.replace(/\/+$/, "").split("/"); p.pop(); setDir(p.join("/") || "/"); };

  return (
    <>
      <div className="toolbar">
        <div className="seg">
          {volumes.map((v) => (
            <button type="button" key={v.mount} className={dir.startsWith(v.mount) && (v.mount !== "/" || crumbs[0].path === "/") ? "active" : ""} onClick={() => setDir(v.mount)} title={v.mount}>
              {v.kind === "external" ? <Usb size={13} /> : <HardDrive size={13} />} {v.name}
            </button>
          ))}
        </div>
        <button type="button" className="btn sm" onClick={() => setDir(home)}><Home size={13} /> {s.home}</button>
        <button type="button" className="btn sm" onClick={up} disabled={dir === "/" || volumes.some((v) => v.mount === dir)}><ArrowUp size={13} /> {s.up}</button>
        <div className="crumbs">
          {crumbs.map((c, i) => (
            <span key={c.path}>{i > 0 && <ChevronRight size={12} />}<button type="button" onClick={() => setDir(c.path)}>{c.name}</button></span>
          ))}
        </div>
        <div className="right">
          <div className="seg">
            <button type="button" className={view === "map" ? "active" : ""} onClick={() => setView("map")}><LayoutGrid size={13} /> {s.treemap}</button>
            <button type="button" className={view === "list" ? "active" : ""} onClick={() => setView("list")}><List size={13} /> {s.listView}</button>
            <button type="button" className={view === "large" ? "active" : ""} onClick={() => setView("large")}><FileStack size={13} /> {s.largeFiles}</button>
          </div>
        </div>
      </div>

      {view === "large" && (
        <section className="panel">
          <div className="panel-h"><span className="ico"><FileStack size={14} /></span>{s.largeFiles}<span className="sub">{largeLoading ? <Loader2 size={13} className="spin" /> : <span className="num">{(large ?? []).length} · {humanSize((large ?? []).reduce((a, n) => a + n.size, 0))}</span>}</span></div>
          <div className="panel-b">
            <p className="help" style={{ color: "var(--ink-3)", fontSize: 12, marginBottom: 10 }}>{s.largeHint}</p>
            <div className="xlist">
              {(large ?? []).map((n) => (
                <div className="lf-row" key={n.path}>
                  <span className="size">{humanSize(n.size)}</span>
                  <span style={{ minWidth: 0 }}><span className="ln">{n.name}</span><span className="lp">{compactPath(n.path, home)}</span></span>
                  <span className="age">{n.modified ? new Date(n.modified).toLocaleDateString() : ""}</span>
                  <button type="button" className="ib" title={s.reveal} onClick={() => onReveal(n.path)}><FolderOpen size={14} /></button>
                  <button type="button" className="ib" title={s.addToSelection} onClick={() => onAdd(n.path)}><FolderPlus size={14} /></button>
                  <button type="button" className="ib" title={s.protectHint} onClick={() => onProtect(n.path.replace(/[\\/][^\\/]+$/, ""))}><ShieldPlus size={14} /></button>
                </div>
              ))}
              {!largeLoading && large && large.length === 0 && <div className="empty"><strong>—</strong></div>}
            </div>
          </div>
        </section>
      )}
      <section className="panel" style={{ position: "relative", display: view === "large" ? "none" : undefined }}>
        <div className="panel-h"><span className="ico"><LayoutGrid size={14} /></span>{compactPath(dir, home)}<span className="sub">{loading ? <Loader2 size={13} className="spin" /> : <span className="num">{humanSize(total)} · {list.length}</span>}</span></div>
        <div className="panel-b">
          {view === "map" && (
            <svg className="treemap" viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" onMouseLeave={tip.hide}>
              {layout.map(({ t, r, kids, color }) => (
                <g key={t.path}>
                  <rect x={r.x + 1} y={r.y + 1} width={Math.max(0, r.w - 2)} height={Math.max(0, r.h - 2)} rx="6" fill={color} fillOpacity="0.16" stroke={color} strokeWidth="1.5"
                    style={{ cursor: t.is_dir ? "pointer" : "default" }}
                    onClick={() => t.is_dir && setDir(t.path)}
                    onMouseEnter={(e) => tip.show(e, <span>{t.name} · <b>{humanSize(t.size)}</b>{t.is_dir ? ` · ${t.files} ${s.files}` : ""}</span>)} onMouseMove={tip.move} />
                  {r.w > 46 && r.h > 20 && (
                    <text x={r.x + 8} y={r.y + 13} fontSize="11" fontWeight="700" fill="var(--ink)" style={{ pointerEvents: "none" }}>
                      {t.name.length > r.w / 7 ? t.name.slice(0, Math.max(3, Math.floor(r.w / 7) - 1)) + "…" : t.name}
                      <tspan fill="var(--ink-3)" fontWeight="500"> {r.w > 120 ? humanSize(t.size) : ""}</tspan>
                    </text>
                  )}
                  {kids.map(({ k, r: kr }) => kr && kr.w > 6 && kr.h > 6 && (
                    <g key={k.path}>
                      <rect x={kr.x + 1} y={kr.y + 1} width={Math.max(0, kr.w - 2)} height={Math.max(0, kr.h - 2)} rx="3" fill={color} fillOpacity="0.35"
                        style={{ cursor: k.is_dir ? "pointer" : "default" }}
                        onClick={(e) => { e.stopPropagation(); if (k.is_dir) setDir(k.path); }}
                        onMouseEnter={(e) => { e.stopPropagation(); tip.show(e, <span>{t.name}/{k.name} · <b>{humanSize(k.size)}</b></span>); }} onMouseMove={tip.move} />
                      {kr.w > 70 && kr.h > 22 && (
                        <text x={kr.x + 7} y={kr.y + 15} fontSize="10.5" fontWeight="600" fill="var(--ink)" style={{ pointerEvents: "none" }}>
                          {k.name.length > kr.w / 7 ? k.name.slice(0, Math.max(3, Math.floor(kr.w / 7) - 1)) + "…" : k.name}
                          <tspan fill="var(--ink-2)" fontWeight="500"> {kr.w > 110 ? humanSize(k.size) : ""}</tspan>
                        </text>
                      )}
                    </g>
                  ))}
                </g>
              ))}
              {layout.length === 0 && !loading && <text x={W / 2} y={H / 2} textAnchor="middle" fill="var(--ink-3)" fontSize="16">—</text>}
            </svg>
          )}
          {view === "list" && (
            <div className="xlist">
              {list.map((n) => (
                <div className="xrow" key={n.path} onDoubleClick={() => n.is_dir && setDir(n.path)}>
                  <span className="xbar"><i style={{ width: `${(n.size / Math.max(1, list[0]?.size ?? 1)) * 100}%` }} /></span>
                  <button type="button" className="xname" onClick={() => n.is_dir && setDir(n.path)} disabled={!n.is_dir}>{n.name}{n.is_dir ? "/" : ""}</button>
                  <span className="xsize num">{humanSize(n.size)}</span>
                  <span className="xfiles num">{n.is_dir ? `${n.files} ${s.files}` : ""}</span>
                  <button type="button" className="ib" title={s.addToSelection} onClick={() => onAdd(n.path)}><FolderPlus size={14} /></button>
                  {n.is_dir && <button type="button" className="ib" title={s.protectHint} onClick={() => onProtect(n.path)}><ShieldPlus size={14} /></button>}
                </div>
              ))}
            </div>
          )}
          <p className="help" style={{ marginTop: 10, color: "var(--ink-3)", fontSize: 12 }}>{s.exploreHint}</p>
        </div>
      </section>
      {tip.el}
    </>
  );
}
