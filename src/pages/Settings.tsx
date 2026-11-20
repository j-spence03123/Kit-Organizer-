import { Download, FolderOutput, Upload } from "lucide-react";
import { useEffect, useState } from "react";
import { PageHeader } from "../components/layout/PageHeader";
import { Button, Field, Panel, Segmented, Toggle, inputClass } from "../components/ui";
import { useApp } from "../hooks/useApp";
import { api, errorMessage } from "../services/api";
import { pickFolders, pickJsonFile, pickSavePath } from "../services/dialogs";
import type { FileTypeSettings, Settings } from "../types";
import { cx } from "../utils/format";

const ACCENTS: [string, string][] = [
  ["amber", "#f5a623"],
  ["teal", "#2dd4bf"],
  ["violet", "#a78bfa"],
  ["rose", "#fb7185"],
  ["sky", "#38bdf8"],
];

const ALL_TYPES: Record<keyof FileTypeSettings, string[]> = {
  audio: ["wav", "mp3", "aif", "aiff", "flac", "ogg"],
  midi: ["mid", "midi"],
  presets: ["fxp", "fxb", "vstpreset", "aupreset", "vital", "serumpreset", "phaseplant", "h2p", "nmsv", "fst", "adv", "adg", "spf", "pgtx"],
};

export default function SettingsPage() {
  const { settings, updateSettings, toast, setConfig } = useApp();
  const [s, setS] = useState<Settings | null>(settings);
  useEffect(() => setS(settings), [settings]);
  if (!s) return null;

  const save = async (next: Settings) => {
    setS(next);
    try {
      await updateSettings(next);
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => void save({ ...s, [k]: v });

  const toggleType = (group: keyof FileTypeSettings, ext: string) => {
    const list = s.fileTypes[group];
    const next = list.includes(ext) ? list.filter((e) => e !== ext) : [...list, ext];
    set("fileTypes", { ...s.fileTypes, [group]: next });
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader title="Settings" subtitle="Changes are saved automatically." />
      <div className="min-h-0 flex-1 overflow-auto px-6 py-5">
        <div className="mx-auto grid max-w-3xl gap-5">
          <Panel title="Organising">
            <div className="grid gap-5 p-4">
              <Field label="Default output folder">
                <div className="flex gap-2">
                  <div className="flex h-8 min-w-0 flex-1 items-center truncate rounded-md border border-line-strong bg-panel-2 px-2.5 font-mono text-[11px] text-muted" data-selectable>
                    {s.defaultOutputDir || "Not set"}
                  </div>
                  <Button
                    icon={<FolderOutput size={14} />}
                    onClick={async () => {
                      const [p] = await pickFolders(false, "Default output folder");
                      if (p) set("defaultOutputDir", p);
                    }}
                  >
                    Choose
                  </Button>
                  {s.defaultOutputDir && <Button variant="ghost" onClick={() => set("defaultOutputDir", null)}>Clear</Button>}
                </div>
              </Field>
              <Field label="Default operation" hint="Move mode always asks for confirmation before running.">
                <Segmented ariaLabel="Default operation" value={s.defaultMode} onChange={(v) => set("defaultMode", v)} options={[{ value: "copy", label: "Copy" }, { value: "move", label: "Move" }]} />
              </Field>
              <Field label="Duplicate files (identical content)" hint="Files with the same name but different content are never treated as duplicates.">
                <Segmented
                  ariaLabel="Duplicate handling"
                  value={s.duplicateHandling}
                  onChange={(v) => set("duplicateHandling", v)}
                  options={[{ value: "skip", label: "Skip duplicates" }, { value: "keep", label: "Keep duplicates" }, { value: "ask", label: "Ask me" }]}
                />
              </Field>
              <Field label="Filename collisions" hint="Existing files are never overwritten.">
                <Segmented
                  ariaLabel="Filename collisions"
                  value={s.collisionHandling}
                  onChange={(v) => set("collisionHandling", v)}
                  options={[{ value: "rename", label: "Rename (Kick 01 (2).wav)" }, { value: "skip", label: "Skip the file" }]}
                />
              </Field>
            </div>
          </Panel>

          <Panel title="Classification">
            <div className="grid gap-5 p-4">
              <Field label={`Confidence threshold: ${Math.round(s.confidenceThreshold * 100)}%`} hint="Samples below this are flagged for review.">
                <input
                  type="range"
                  min={0}
                  max={0.95}
                  step={0.05}
                  value={s.confidenceThreshold}
                  onChange={(e) => setS({ ...s, confidenceThreshold: Number(e.target.value) })}
                  onMouseUp={() => void save(s)}
                  onKeyUp={() => void save(s)}
                  aria-label="Confidence threshold"
                  className="accent-[var(--accent)]"
                />
              </Field>
              <Field label="Unreviewed low-confidence samples when organising">
                <Segmented
                  ariaLabel="Low confidence action"
                  value={s.lowConfidenceAction}
                  onChange={(v) => set("lowConfidenceAction", v)}
                  options={[
                    { value: "uncategorised", label: "Put in Uncategorised" },
                    { value: "predicted", label: "Use prediction" },
                    { value: "skip", label: "Leave for later" },
                  ]}
                />
              </Field>
              <Field label="Worker threads" hint="Used for hashing and reading audio headers. 0 = automatic.">
                <input type="number" min={0} max={32} className={cx(inputClass, "w-24")} value={s.workerThreads} onChange={(e) => set("workerThreads", Math.max(0, Math.min(32, Number(e.target.value) || 0)))} aria-label="Worker threads" />
              </Field>
            </div>
          </Panel>

          <Panel title="Supported file types">
            <div className="grid gap-4 p-4">
              {(Object.keys(ALL_TYPES) as (keyof FileTypeSettings)[]).map((g) => (
                <div key={g}>
                  <div className="mb-1.5 text-xs font-medium capitalize text-muted">{g === "presets" ? "Presets" : g === "midi" ? "MIDI" : "Audio"}</div>
                  <div className="flex flex-wrap gap-1.5">
                    {ALL_TYPES[g].map((ext) => {
                      const on = s.fileTypes[g].includes(ext);
                      return (
                        <button
                          key={ext}
                          type="button"
                          aria-pressed={on}
                          onClick={() => toggleType(g, ext)}
                          className={cx("rounded border px-2 py-0.5 font-mono text-[11px]", on ? "border-accent/50 bg-accent-soft text-fg" : "border-line-strong text-faint")}
                        >
                          .{ext}
                        </button>
                      );
                    })}
                  </div>
                </div>
              ))}
              <p className="text-xs text-faint">Disabled and unknown types are counted during scanning but never moved, copied or deleted.</p>
            </div>
          </Panel>

          <Panel title="Audio preview">
            <div className="grid gap-4 p-4">
              <Field label={`Volume: ${Math.round(s.preview.volume * 100)}%`}>
                <input type="range" min={0} max={1} step={0.05} value={s.preview.volume} onChange={(e) => set("preview", { ...s.preview, volume: Number(e.target.value) })} aria-label="Preview volume" className="accent-[var(--accent)]" />
              </Field>
              <Toggle label="Loop playback by default" checked={s.preview.loopPlayback} onChange={(v) => set("preview", { ...s.preview, loopPlayback: v })} />
            </div>
          </Panel>

          <Panel title="Appearance">
            <div className="grid gap-4 p-4">
              <Field label="Accent colour">
                <div className="flex gap-2" role="radiogroup" aria-label="Accent colour">
                  {ACCENTS.map(([name, color]) => (
                    <button
                      key={name}
                      type="button"
                      role="radio"
                      aria-checked={s.appearance.accent === name}
                      aria-label={name}
                      onClick={() => set("appearance", { ...s.appearance, accent: name })}
                      className={cx("h-7 w-7 rounded-full border-2", s.appearance.accent === name ? "border-fg" : "border-transparent")}
                      style={{ background: color }}
                    />
                  ))}
                </div>
              </Field>
              <Toggle label="Compact table rows" checked={s.appearance.compactRows} onChange={(v) => set("appearance", { ...s.appearance, compactRows: v })} />
            </div>
          </Panel>

          <Panel title="Configuration backup">
            <div className="flex flex-wrap items-center gap-2 p-4">
              <Button
                icon={<Download size={14} />}
                onClick={async () => {
                  const p = await pickSavePath("kit-organizer-config.json");
                  if (!p) return;
                  try {
                    await api.exportConfig(p, true);
                    toast("Settings and categories exported", "ok");
                  } catch (e) {
                    toast(errorMessage(e), "error");
                  }
                }}
              >
                Export settings & categories
              </Button>
              <Button
                icon={<Upload size={14} />}
                onClick={async () => {
                  const p = await pickJsonFile();
                  if (!p) return;
                  try {
                    const cfg = await api.importConfig(p);
                    await api.saveCategoryConfig(cfg.categories);
                    setConfig(cfg.categories);
                    if (cfg.settings) await updateSettings(cfg.settings);
                    toast("Configuration imported and applied", "ok");
                  } catch (e) {
                    toast(errorMessage(e), "error");
                  }
                }}
              >
                Import configuration
              </Button>
            </div>
          </Panel>
        </div>
      </div>
    </div>
  );
}
