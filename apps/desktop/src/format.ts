export function humanSize(bytes: number): string {
  if (!bytes || bytes < 1024) return `${Math.max(0, Math.round(bytes || 0))} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const digits = v >= 100 ? 0 : v >= 10 ? 1 : 2;
  return `${v.toFixed(digits)} ${units[i]}`;
}

export function compactPath(path: string, home: string): string {
  if (home && path.startsWith(home)) {
    const rest = path.slice(home.length);
    return rest ? `~${rest}` : "~";
  }
  return path;
}

export function ageDays(iso?: string | null): number | null {
  if (!iso) return null;
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return null;
  return Math.max(0, Math.floor((Date.now() - t) / 86_400_000));
}

export function ageLabel(days: number | null, lang: "en" | "tr"): string {
  if (days === null) return "—";
  if (days === 0) return lang === "tr" ? "bugün" : "today";
  if (days < 30) return lang === "tr" ? `${days} gün` : `${days}d`;
  if (days < 365) return lang === "tr" ? `${Math.floor(days / 30)} ay` : `${Math.floor(days / 30)}mo`;
  return lang === "tr" ? `${Math.floor(days / 365)} yıl` : `${Math.floor(days / 365)}y`;
}

export function formatDate(iso: string, lang: "en" | "tr"): string {
  const d = new Date(iso);
  return d.toLocaleString(lang === "tr" ? "tr-TR" : "en-GB", {
    year: "numeric",
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function pct(part: number, total: number): number {
  return total > 0 ? (part / total) * 100 : 0;
}
