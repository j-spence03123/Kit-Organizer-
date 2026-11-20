import { ArrowDown, ArrowUp, Pause, Play } from "lucide-react";
import type { ReactNode } from "react";
import { usePlayer } from "../../hooks/useAudioPlayer";
import type { SampleRow } from "../../types";
import { cx } from "../../utils/format";
import { ConfidenceBar } from "../ui";
import { StatusBadge } from "./status";

export type ColumnKey = "filename" | "folder" | "category" | "confidence" | "destination" | "status" | "pack";

const LABELS: Record<ColumnKey, string> = {
  filename: "Filename",
  folder: "Original folder",
  category: "Predicted category",
  confidence: "Confidence",
  destination: "Destination",
  status: "Status",
  pack: "Source pack",
};

const WIDTHS: Record<ColumnKey, string> = {
  filename: "minmax(150px,1.4fr)",
  folder: "minmax(90px,1fr)",
  category: "minmax(120px,1fr)",
  confidence: "96px",
  destination: "minmax(100px,1fr)",
  status: "116px",
  pack: "minmax(100px,0.8fr)",
};

interface Props {
  rows: SampleRow[];
  columns: ColumnKey[];
  threshold: number;
  rootFolder: string;
  selected?: Set<string>;
  focusedId: string | null;
  onFocus: (row: SampleRow) => void;
  /** Select without side effects (used by the row play button). */
  onSelect?: (row: SampleRow) => void;
  onToggle?: (row: SampleRow, shift: boolean) => void;
  onToggleAll?: () => void;
  sort: string;
  descending: boolean;
  onSort: (key: string) => void;
  compact?: boolean;
  empty?: ReactNode;
}

function destination(r: SampleRow, root: string): string {
  if (r.destinationPath) return r.destinationPath;
  return `${root}/${r.categoryPath}/${r.originalFilename}`;
}

export function SampleTable(props: Props) {
  const { rows, columns, threshold, rootFolder, selected, focusedId, onFocus, onSelect, onToggle, onToggleAll, sort, descending, onSort, compact, empty } = props;
  const player = usePlayer();
  const selectable = !!selected && !!onToggle;
  const template = [selectable ? "32px" : null, "32px", ...columns.map((c) => WIDTHS[c])].filter(Boolean).join(" ");
  const allSelected = selectable && rows.length > 0 && rows.every((r) => selected.has(r.id));
  const rowH = compact ? "h-8" : "h-10";

  return (
    <div role="table" aria-label="Samples" aria-rowcount={rows.length} className="min-w-[720px] text-[13px]">
      <div role="row" className="sticky top-0 z-10 grid items-center border-b border-line bg-panel px-2 text-xs text-muted" style={{ gridTemplateColumns: template }}>
        {selectable && (
          <div role="columnheader" className="flex justify-center">
            <input type="checkbox" aria-label="Select all on this page" checked={allSelected} onChange={onToggleAll} className="kit-check" />
          </div>
        )}
        <div role="columnheader" />
        {columns.map((c) => (
          <div role="columnheader" key={c} aria-sort={sort === c ? (descending ? "descending" : "ascending") : "none"}>
            {c === "destination" ? (
              <span className="px-2 py-2">{LABELS[c]}</span>
            ) : (
              <button type="button" className="flex items-center gap-1 px-2 py-2 hover:text-fg" onClick={() => onSort(c)}>
                {LABELS[c]}
                {sort === c && (descending ? <ArrowDown size={11} /> : <ArrowUp size={11} />)}
              </button>
            )}
          </div>
        ))}
      </div>
      {rows.length === 0 && empty}
      {rows.map((r) => {
        const path = r.status === "organised" && r.destinationPath ? r.destinationPath : r.originalPath;
        const playing = player.path === path && player.status === "playing";
        const loading = player.path === path && player.status === "loading";
        return (
          <div
            role="row"
            key={r.id}
            aria-selected={focusedId === r.id}
            onClick={(e) => {
              if (selectable && (e.metaKey || e.ctrlKey || e.shiftKey)) onToggle(r, e.shiftKey);
              else onFocus(r);
            }}
            className={cx(
              "grid cursor-default items-center border-b border-line/60 px-2",
              rowH,
              focusedId === r.id ? "bg-accent-soft" : selected?.has(r.id) ? "bg-raised/70" : "hover:bg-panel-2",
            )}
            style={{ gridTemplateColumns: template }}
          >
            {selectable && (
              <div role="cell" className="flex justify-center" onClick={(e) => e.stopPropagation()}>
                <input
                  type="checkbox"
                  aria-label={`Select ${r.originalFilename}`}
                  checked={selected.has(r.id)}
                  onChange={(e) => onToggle(r, (e.nativeEvent as MouseEvent).shiftKey)}
                  className="kit-check"
                />
              </div>
            )}
            <div role="cell" className="flex justify-center">
              {r.fileKind === "audio" && (
                <button
                  type="button"
                  aria-label={playing ? `Pause ${r.originalFilename}` : `Play ${r.originalFilename}`}
                  className={cx("flex h-6 w-6 items-center justify-center rounded-full hover:bg-raised", playing || loading ? "text-accent" : "text-faint hover:text-fg")}
                  onClick={(e) => {
                    e.stopPropagation();
                    (onSelect ?? onFocus)(r);
                    void player.toggle(path);
                  }}
                >
                  {playing ? <Pause size={12} /> : <Play size={12} />}
                </button>
              )}
            </div>
            {columns.map((c) => (
              <div role="cell" key={c} className="min-w-0 truncate px-2">
                {c === "filename" && <span title={r.originalFilename} className="text-fg">{r.originalFilename}</span>}
                {c === "folder" && <span title={r.relativeDir} className="text-muted">{r.relativeDir}</span>}
                {c === "category" && <span title={r.categoryPath}>{r.categoryPath}</span>}
                {c === "confidence" && <ConfidenceBar value={r.confidence} threshold={threshold} />}
                {c === "destination" && (
                  <span title={destination(r, rootFolder)} className="font-mono text-[11px] text-faint">
                    {r.destinationPath ?? `…/${r.categoryPath}/${r.originalFilename}`}
                  </span>
                )}
                {c === "status" && <StatusBadge row={r} threshold={threshold} />}
                {c === "pack" && <span className="text-muted">{r.sourcePack}</span>}
              </div>
            ))}
          </div>
        );
      })}
    </div>
  );
}

export function Pager({ offset, limit, total, onChange }: { offset: number; limit: number; total: number; onChange: (offset: number) => void }) {
  const page = Math.floor(offset / limit) + 1;
  const pages = Math.max(1, Math.ceil(total / limit));
  return (
    <div className="flex items-center gap-2 text-xs text-muted">
      <span className="tabular">
        {total === 0 ? "0" : `${offset + 1}–${Math.min(total, offset + limit)}`} of {total.toLocaleString()}
      </span>
      <button type="button" className="rounded px-2 py-1 hover:bg-raised disabled:opacity-30" disabled={page <= 1} onClick={() => onChange(Math.max(0, offset - limit))}>
        Prev
      </button>
      <span className="tabular">
        {page}/{pages}
      </span>
      <button type="button" className="rounded px-2 py-1 hover:bg-raised disabled:opacity-30" disabled={page >= pages} onClick={() => onChange(offset + limit)}>
        Next
      </button>
    </div>
  );
}
