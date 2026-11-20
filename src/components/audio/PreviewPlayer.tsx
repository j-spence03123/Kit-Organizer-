import { Pause, Play, Repeat, Square } from "lucide-react";
import { useState } from "react";
import { usePlayer } from "../../hooks/useAudioPlayer";
import type { AudioMeta } from "../../types";
import { formatDuration } from "../../utils/format";
import { IconButton } from "../ui";
import { Waveform } from "./Waveform";

interface Props {
  path: string;
  name: string;
  audio: AudioMeta | null;
  bpm: number | null;
  bpmSource: string | null;
  musicalKey: string | null;
  playable: boolean;
}

/** Transport, waveform and metadata for one sample. Values that are not
 * reliably known are shown as "Unknown" — never estimated. */
export function PreviewPlayer({ path, name, audio, bpm, bpmSource, musicalKey, playable }: Props) {
  const p = usePlayer();
  const [loop, setLoop] = useState(false);
  const active = p.path === path;
  const playing = active && p.status === "playing";
  const progress = active && p.duration > 0 ? p.position / p.duration : 0;

  const meta: [string, string][] = [
    ["Duration", formatDuration(audio?.durationSecs ?? (active && p.duration ? p.duration : null))],
    ["Sample rate", audio?.sampleRate ? `${(audio.sampleRate / 1000).toFixed(1)} kHz` : "Unknown"],
    ["Channels", audio?.channels ? (audio.channels === 1 ? "Mono" : audio.channels === 2 ? "Stereo" : String(audio.channels)) : "Unknown"],
    ["Bit depth", audio?.bitDepth ? `${audio.bitDepth}-bit` : "Unknown"],
    ["BPM", bpm ? `${Number.isInteger(bpm) ? bpm : bpm.toFixed(2)}${bpmSource ? ` (${bpmSource})` : ""}` : "Unknown"],
    ["Key", musicalKey ? `${musicalKey} (filename)` : "Unknown"],
  ];

  return (
    <div className="grid min-w-0 gap-3">
      <div className="flex items-center gap-2">
        <IconButton
          label={playing ? "Pause" : "Play"}
          disabled={!playable}
          onClick={() => void p.toggle(path)}
          className="h-8 w-8 rounded-full bg-accent text-black hover:bg-accent-strong hover:text-black"
        >
          {playing ? <Pause size={15} /> : <Play size={15} className="translate-x-px" />}
        </IconButton>
        <IconButton label="Stop" disabled={!active} onClick={p.stop}>
          <Square size={13} />
        </IconButton>
        <IconButton
          label="Loop playback"
          aria-pressed={loop}
          onClick={() => {
            setLoop(!loop);
            p.setLoop(!loop);
          }}
          className={loop ? "text-accent" : undefined}
        >
          <Repeat size={14} />
        </IconButton>
        <div className="min-w-0 flex-1 truncate text-[13px] font-medium" title={name} data-selectable>
          {name}
        </div>
        <span className="tabular shrink-0 text-xs text-muted">
          {active ? formatDuration(p.position) : "0.00s"} / {formatDuration(active && p.duration ? p.duration : audio?.durationSecs)}
        </span>
      </div>
      {playable ? (
        <Waveform peaks={active ? p.peaks : null} progress={progress} onSeek={active ? p.seek : undefined} loading={active && p.status === "loading"} />
      ) : (
        <div className="flex h-14 items-center justify-center rounded border border-dashed border-line text-xs text-faint">Preview is available for audio files only</div>
      )}
      {active && p.status === "error" && <p className="text-xs text-bad">Could not play this file: {p.error}</p>}
      {audio?.error && <p className="text-xs text-warn">Header could not be read: {audio.error}</p>}
      <dl className="grid grid-cols-3 gap-x-4 gap-y-2">
        {meta.map(([k, v]) => (
          <div key={k}>
            <dt className="text-[11px] text-faint">{k}</dt>
            <dd className={v === "Unknown" ? "text-xs text-faint" : "tabular text-xs text-fg"}>{v}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
