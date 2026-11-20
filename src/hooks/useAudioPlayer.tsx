// One shared audio engine for the whole app. Files are fetched through the
// `kitaudio://` protocol (decoded to WAV in Rust), decoded with Web Audio
// and played through a gain node. A generation counter guarantees that
// rapidly switching samples never lets a stale load start playing.
import { convertFileSrc } from "@tauri-apps/api/core";
import { createContext, type ReactNode, useCallback, useContext, useEffect, useRef, useState } from "react";

export type PlayerStatus = "idle" | "loading" | "playing" | "paused" | "error";

interface Loaded {
  path: string;
  buffer: AudioBuffer;
  peaks: Float32Array;
}

interface PlayerValue {
  path: string | null;
  status: PlayerStatus;
  error: string | null;
  duration: number;
  position: number;
  peaks: Float32Array | null;
  play: (path: string) => Promise<void>;
  toggle: (path: string) => Promise<void>;
  stop: () => void;
  seek: (fraction: number) => void;
  setVolume: (v: number) => void;
  setLoop: (v: boolean) => void;
}

const PlayerContext = createContext<PlayerValue | null>(null);

const PEAK_BUCKETS = 800;
const CACHE_SIZE = 24;

export function computePeaks(buffer: AudioBuffer, buckets = PEAK_BUCKETS): Float32Array {
  const out = new Float32Array(buckets);
  const len = buffer.length;
  const per = Math.max(1, Math.floor(len / buckets));
  for (let c = 0; c < buffer.numberOfChannels; c++) {
    const data = buffer.getChannelData(c);
    for (let b = 0; b < buckets; b++) {
      let max = out[b] ?? 0;
      const start = b * per;
      const end = Math.min(len, start + per);
      for (let i = start; i < end; i += 4) {
        const v = Math.abs(data[i] ?? 0);
        if (v > max) max = v;
      }
      out[b] = max;
    }
  }
  return out;
}

export function previewUrl(path: string): string {
  return convertFileSrc(path, "kitaudio");
}

export function AudioPlayerProvider({ children, volume = 0.8, loop = false }: { children: ReactNode; volume?: number; loop?: boolean }) {
  const ctxRef = useRef<AudioContext | null>(null);
  const gainRef = useRef<GainNode | null>(null);
  const sourceRef = useRef<AudioBufferSourceNode | null>(null);
  const cache = useRef(new Map<string, Loaded>());
  const generation = useRef(0);
  const startedAt = useRef(0);
  const offset = useRef(0);
  const loaded = useRef<Loaded | null>(null);
  const loopRef = useRef(loop);
  const raf = useRef<number | null>(null);

  const [path, setPath] = useState<string | null>(null);
  const [status, setStatus] = useState<PlayerStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const [duration, setDuration] = useState(0);
  const [position, setPosition] = useState(0);
  const [peaks, setPeaks] = useState<Float32Array | null>(null);

  const ensureCtx = () => {
    if (!ctxRef.current) {
      const ctx = new AudioContext();
      const gain = ctx.createGain();
      gain.gain.value = volume;
      gain.connect(ctx.destination);
      ctxRef.current = ctx;
      gainRef.current = gain;
    }
    return ctxRef.current;
  };

  const tick = useCallback(() => {
    const ctx = ctxRef.current;
    const l = loaded.current;
    if (ctx && l && sourceRef.current) {
      let pos = offset.current + (ctx.currentTime - startedAt.current);
      if (loopRef.current) pos = pos % l.buffer.duration;
      setPosition(Math.min(pos, l.buffer.duration));
      raf.current = requestAnimationFrame(tick);
    }
  }, []);

  const stopSource = useCallback(() => {
    const s = sourceRef.current;
    sourceRef.current = null;
    if (s) {
      s.onended = null;
      try {
        s.stop();
      } catch {
        /* already stopped */
      }
      s.disconnect();
    }
    if (raf.current) cancelAnimationFrame(raf.current);
    raf.current = null;
  }, []);

  const startAt = useCallback(
    (from: number) => {
      const ctx = ensureCtx();
      const l = loaded.current;
      if (!l || !gainRef.current) return;
      stopSource();
      const src = ctx.createBufferSource();
      src.buffer = l.buffer;
      src.loop = loopRef.current;
      src.connect(gainRef.current);
      src.onended = () => {
        if (sourceRef.current === src) {
          sourceRef.current = null;
          offset.current = 0;
          setPosition(0);
          setStatus("paused");
        }
      };
      offset.current = from;
      startedAt.current = ctx.currentTime;
      src.start(0, from);
      sourceRef.current = src;
      setStatus("playing");
      raf.current = requestAnimationFrame(tick);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [stopSource, tick],
  );

  const play = useCallback(
    async (p: string) => {
      const gen = ++generation.current;
      stopSource();
      setPath(p);
      setError(null);
      setPosition(0);
      const ctx = ensureCtx();
      if (ctx.state === "suspended") await ctx.resume();
      let entry = cache.current.get(p);
      if (!entry) {
        setStatus("loading");
        setPeaks(null);
        try {
          const res = await fetch(previewUrl(p));
          if (!res.ok) throw new Error((await res.text()) || `HTTP ${res.status}`);
          const data = await res.arrayBuffer();
          if (gen !== generation.current) return;
          const buffer = await ctx.decodeAudioData(data);
          if (gen !== generation.current) return;
          entry = { path: p, buffer, peaks: computePeaks(buffer) };
          cache.current.set(p, entry);
          if (cache.current.size > CACHE_SIZE) {
            const first = cache.current.keys().next().value;
            if (first !== undefined) cache.current.delete(first);
          }
        } catch (e) {
          if (gen !== generation.current) return;
          setStatus("error");
          setError(e instanceof Error ? e.message : String(e));
          return;
        }
      }
      if (gen !== generation.current) return;
      loaded.current = entry;
      setPeaks(entry.peaks);
      setDuration(entry.buffer.duration);
      startAt(0);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [startAt, stopSource],
  );

  const stop = useCallback(() => {
    generation.current++;
    stopSource();
    offset.current = 0;
    setPosition(0);
    setStatus((s) => (s === "idle" ? s : "paused"));
  }, [stopSource]);

  const toggle = useCallback(
    async (p: string) => {
      if (p !== path || !loaded.current || loaded.current.path !== p) return play(p);
      if (status === "playing") {
        const ctx = ctxRef.current;
        const pos = ctx ? offset.current + (ctx.currentTime - startedAt.current) : 0;
        stopSource();
        offset.current = loaded.current ? pos % loaded.current.buffer.duration : 0;
        setStatus("paused");
      } else {
        startAt(offset.current);
      }
    },
    [path, status, play, stopSource, startAt],
  );

  const seek = useCallback(
    (fraction: number) => {
      const l = loaded.current;
      if (!l) return;
      const t = Math.max(0, Math.min(1, fraction)) * l.buffer.duration;
      setPosition(t);
      if (status === "playing") startAt(t);
      else offset.current = t;
    },
    [status, startAt],
  );

  const setVolume = useCallback((v: number) => {
    if (gainRef.current) gainRef.current.gain.value = v;
  }, []);
  const setLoop = useCallback((v: boolean) => {
    loopRef.current = v;
    if (sourceRef.current) sourceRef.current.loop = v;
  }, []);

  useEffect(() => setVolume(volume), [volume, setVolume]);
  useEffect(() => setLoop(loop), [loop, setLoop]);
  useEffect(() => () => {
    stopSource();
    void ctxRef.current?.close();
  }, [stopSource]);

  return (
    <PlayerContext.Provider value={{ path, status, error, duration, position, peaks, play, toggle, stop, seek, setVolume, setLoop }}>
      {children}
    </PlayerContext.Provider>
  );
}

export function usePlayer(): PlayerValue {
  const v = useContext(PlayerContext);
  if (!v) throw new Error("usePlayer must be used inside AudioPlayerProvider");
  return v;
}
