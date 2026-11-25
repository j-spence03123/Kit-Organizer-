import { Check, Copy as CopyIcon, Package, Pencil } from "lucide-react";
import { useState } from "react";
import type { SampleRow } from "../../types";
import { formatBytes, percent } from "../../utils/format";
import { PreviewPlayer } from "../audio/PreviewPlayer";
import { Badge, Button, inputClass } from "../ui";
import { CategoryPicker } from "./CategoryPicker";
import { StatusBadge } from "./status";

/** Embedded tags and signal-analysis measurements, when available. */
function AudioInsights({ audio }: { audio: SampleRow["audio"] }) {
  if (!audio) return null;
  const tags = [audio.title && `Title: ${audio.title}`, audio.genre && `Genre: ${audio.genre}`, audio.comment && `Comment: ${audio.comment}`].filter(Boolean) as string[];
  const f = audio.features;
  if (tags.length === 0 && !f) return null;
  return (
    <div className="grid gap-1 text-[11px] text-muted" data-selectable>
      {tags.map((t) => (
        <div key={t} className="truncate" title={t}>{t}</div>
      ))}
      {f && (
        <div title="Simple signal measurements used as supporting evidence — not sound recognition.">
          Analysis: {Math.round(f.lowRatio * 100)}% low end · {f.onsets} hit{f.onsets === 1 ? "" : "s"}
          {f.decayMs != null ? ` · decays in ${(f.decayMs / 1000).toFixed(2)}s` : " · sustained"}
          {f.periodicity > 0.45 ? " · rhythmic" : ""}
          {f.riseDb > 12 ? ` · rising +${Math.round(f.riseDb)} dB` : ""}
        </div>
      )}
    </div>
  );
}

interface Props {
  row: SampleRow;
  threshold: number;
  editable: boolean;
  onReassign: (ids: string[], categoryId: string) => void;
  onAccept: (ids: string[]) => void;
  onDuplicate: (ids: string[], d: "keep" | "skip") => void;
  onSourcePack: (ids: string[], pack: string) => void;
}

export function SampleDetail({ row, threshold, editable, onReassign, onAccept, onDuplicate, onSourcePack }: Props) {
  const [editingPack, setEditingPack] = useState(false);
  const [pack, setPack] = useState(row.sourcePack);
  const path = row.status === "organised" && row.destinationPath ? row.destinationPath : row.originalPath;
  return (
    <div className="grid min-w-0 gap-4 p-4">
      <PreviewPlayer
        path={path}
        name={row.originalFilename}
        audio={row.audio}
        bpm={row.bpm}
        bpmSource={row.bpmSource}
        musicalKey={row.key}
        keySource={row.keySource}
        playable={row.fileKind === "audio"}
      />
      <AudioInsights audio={row.audio} />

      <section className="grid gap-2 border-t border-line pt-3">
        <div className="flex items-center justify-between">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-faint">Classification</h3>
          <StatusBadge row={row} threshold={threshold} />
        </div>
        <div className="text-sm font-medium text-fg" data-selectable>
          {row.categoryPath}
          {row.subfolder && <span className="text-muted"> / {row.subfolder}</span>}
        </div>
        <div className="flex flex-wrap gap-1.5">
          <Badge tone={row.confidence >= threshold ? "ok" : "warn"}>{percent(row.confidence)} confidence</Badge>
          <Badge>{row.sampleType}</Badge>
          <Badge>{row.manualOverride ? "manual" : row.method}</Badge>
        </div>
        {editable && (
          <div className="flex flex-wrap gap-2 pt-1">
            <CategoryPicker onSelect={(id) => onReassign([row.id], id)} label="Change category" />
            {!row.reviewed && (
              <Button size="sm" icon={<Check size={13} />} onClick={() => onAccept([row.id])}>
                Accept
              </Button>
            )}
          </div>
        )}
        {row.alternatives.length > 0 && (
          <div className="grid gap-1 pt-1">
            <span className="text-[11px] text-faint">Alternatives</span>
            <div className="flex flex-wrap gap-1.5">
              {row.alternatives.map((a) => (
                <button
                  key={a.categoryId}
                  type="button"
                  disabled={!editable}
                  onClick={() => onReassign([row.id], a.categoryId)}
                  className="rounded border border-line-strong px-1.5 py-0.5 text-[11px] text-muted hover:border-accent hover:text-fg disabled:pointer-events-none"
                  title={`Score ${a.score}`}
                >
                  {a.path}
                </button>
              ))}
            </div>
          </div>
        )}
        <details className="group pt-1" open>
          <summary className="cursor-pointer text-[11px] text-faint hover:text-muted">Evidence ({row.evidence.length})</summary>
          <ul className="mt-1.5 grid gap-1" data-selectable>
            {row.evidence.map((e, i) => (
              <li key={i} className="rounded bg-panel-2 px-2 py-1 font-mono text-[11px] leading-4 text-muted">
                {e}
              </li>
            ))}
          </ul>
        </details>
      </section>

      {row.duplicateOf && (
        <section className="grid gap-2 border-t border-line pt-3">
          <h3 className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-warn">
            <CopyIcon size={12} /> Identical content
          </h3>
          <p className="break-all text-xs text-muted" data-selectable>
            Same audio data as <span className="font-mono text-fg">{row.duplicatePath}</span>
          </p>
          {editable && (
            <div className="flex gap-2">
              <Button size="sm" variant={row.dupDecision === "skip" ? "primary" : "secondary"} onClick={() => onDuplicate([row.id], "skip")}>
                Skip duplicate
              </Button>
              <Button size="sm" variant={row.dupDecision === "keep" ? "primary" : "secondary"} onClick={() => onDuplicate([row.id], "keep")}>
                Keep both
              </Button>
            </div>
          )}
        </section>
      )}

      <section className="grid gap-2 border-t border-line pt-3 text-xs">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-faint">Provenance</h3>
        <div className="flex items-center gap-2">
          <Package size={13} className="text-faint" />
          {editingPack ? (
            <form
              className="flex flex-1 gap-1.5"
              onSubmit={(e) => {
                e.preventDefault();
                onSourcePack([row.id], pack);
                setEditingPack(false);
              }}
            >
              <input className={inputClass} value={pack} onChange={(e) => setPack(e.target.value)} aria-label="Source pack" autoFocus />
              <Button size="sm" type="submit">Save</Button>
            </form>
          ) : (
            <>
              <span className="text-fg">{row.sourcePack}</span>
              <button type="button" className="text-faint hover:text-fg" aria-label="Edit source pack" onClick={() => setEditingPack(true)}>
                <Pencil size={12} />
              </button>
            </>
          )}
        </div>
        <dl className="grid gap-1.5" data-selectable>
          <div>
            <dt className="text-faint">Original file</dt>
            <dd className="break-all font-mono text-[11px] text-muted">{row.originalPath}</dd>
          </div>
          {row.destinationPath && (
            <div>
              <dt className="text-faint">Organised to</dt>
              <dd className="break-all font-mono text-[11px] text-muted">{row.destinationPath}</dd>
            </div>
          )}
          <div className="flex gap-6">
            <div>
              <dt className="text-faint">Size</dt>
              <dd className="text-muted">{formatBytes(row.size)}</dd>
            </div>
            <div className="min-w-0">
              <dt className="text-faint">BLAKE3</dt>
              <dd className="truncate font-mono text-[11px] text-muted" title={row.hash ?? ""}>{row.hash?.slice(0, 16) ?? "—"}</dd>
            </div>
          </div>
          {row.error && (
            <div>
              <dt className="text-faint">Error</dt>
              <dd className="text-bad">{row.error}</dd>
            </div>
          )}
        </dl>
      </section>
    </div>
  );
}
