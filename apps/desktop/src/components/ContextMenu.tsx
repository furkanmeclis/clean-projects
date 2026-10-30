import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  danger?: boolean;
  disabled?: boolean;
  separator?: boolean;
  onClick?: () => void;
}

export interface MenuState {
  x: number;
  y: number;
  items: MenuItem[];
}

/** Right-click menu anchored at a screen position; closes on click, Escape, scroll or resize. */
export function ContextMenu({ menu, onClose }: { menu: MenuState | null; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  useLayoutEffect(() => {
    if (!menu || !ref.current) { setPos(null); return; }
    const r = ref.current.getBoundingClientRect();
    const x = Math.min(menu.x, window.innerWidth - r.width - 8);
    const y = Math.min(menu.y, window.innerHeight - r.height - 8);
    setPos({ x: Math.max(8, x), y: Math.max(8, y) });
  }, [menu]);
  useEffect(() => {
    if (!menu) return;
    const close = () => onClose();
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("mousedown", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    window.addEventListener("keydown", key);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("keydown", key);
    };
  }, [menu, onClose]);
  if (!menu) return null;
  return (
    <div ref={ref} className="ctx" role="menu" style={{ left: pos?.x ?? menu.x, top: pos?.y ?? menu.y, visibility: pos ? "visible" : "hidden" }} onMouseDown={(e) => e.stopPropagation()} onContextMenu={(e) => e.preventDefault()}>
      {menu.items.map((it, i) =>
        it.separator ? (
          <div className="ctx-sep" key={i} />
        ) : (
          <button type="button" key={i} role="menuitem" className={`ctx-item ${it.danger ? "danger" : ""}`} disabled={it.disabled} onClick={() => { it.onClick?.(); onClose(); }}>
            <span className="ctx-ico">{it.icon}</span>{it.label}
          </button>
        ),
      )}
    </div>
  );
}

/** Helper to open a menu from an onContextMenu handler. */
export function openMenu(e: React.MouseEvent, items: MenuItem[], set: (m: MenuState) => void) {
  e.preventDefault();
  e.stopPropagation();
  set({ x: e.clientX, y: e.clientY, items });
}
