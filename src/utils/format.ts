export function formatBytes(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return "Unknown";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v < 10 && i > 0 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

export function formatDuration(secs: number | null | undefined): string {
  if (secs == null || !Number.isFinite(secs)) return "Unknown";
  if (secs < 10) return `${secs.toFixed(2)}s`;
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return m > 0 ? `${m}:${String(s).padStart(2, "0")}` : `${secs.toFixed(1)}s`;
}

export function formatNumber(n: number): string {
  return new Intl.NumberFormat().format(n);
}

export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

export function percent(v: number): string {
  return `${Math.round(v * 100)}%`;
}

export function basename(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? p;
}

export function joinPath(...parts: string[]): string {
  const sep = parts.some((p) => p.includes("\\")) && !parts.some((p) => p.includes("/")) ? "\\" : "/";
  return parts
    .filter(Boolean)
    .map((p, i) => (i === 0 ? p.replace(/[\\/]+$/, "") : p.replace(/^[\\/]+|[\\/]+$/g, "")))
    .join(sep);
}

export function cx(...classes: (string | false | null | undefined)[]): string {
  return classes.filter(Boolean).join(" ");
}
