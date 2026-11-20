import { useEffect, useRef } from "react";

/** Canvas waveform with playback progress; click to seek. */
export function Waveform({
  peaks,
  progress,
  onSeek,
  height = 56,
  loading,
}: {
  peaks: Float32Array | null;
  progress: number;
  onSeek?: (fraction: number) => void;
  height?: number;
  loading?: boolean;
}) {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    canvas.width = Math.max(1, Math.floor(w * dpr));
    canvas.height = Math.floor(height * dpr);
    const g = canvas.getContext("2d");
    if (!g) return;
    g.scale(dpr, dpr);
    g.clearRect(0, 0, w, height);
    const styles = getComputedStyle(document.documentElement);
    const accent = styles.getPropertyValue("--accent").trim() || "#f5a623";
    const mid = height / 2;
    if (!peaks || peaks.length === 0) {
      g.fillStyle = "#2c2c32";
      g.fillRect(0, mid - 0.5, w, 1);
      return;
    }
    const bar = 2;
    const gap = 1;
    const n = Math.floor(w / (bar + gap));
    let max = 0.0001;
    for (const p of peaks) if (p > max) max = p;
    for (let i = 0; i < n; i++) {
      const idx = Math.floor((i / n) * peaks.length);
      const v = (peaks[idx] ?? 0) / max;
      const h = Math.max(1, v * (height - 4));
      const x = i * (bar + gap);
      g.fillStyle = x / w <= progress ? accent : "#4a4a52";
      g.fillRect(x, mid - h / 2, bar, h);
    }
  }, [peaks, progress, height]);

  return (
    <div className="relative w-full min-w-0" style={{ height }}>
      {/* Absolutely positioned so the canvas's pixel width never stretches its container. */}
      <canvas
        ref={ref}
        className="absolute inset-0 h-full w-full cursor-pointer"
        aria-label="Waveform, click to seek"
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          onSeek?.((e.clientX - r.left) / r.width);
        }}
      />
      {loading && <div className="absolute inset-0 flex items-center justify-center text-xs text-muted">Decoding…</div>}
    </div>
  );
}
