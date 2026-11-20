import { Loader2 } from "lucide-react";
import type { ScanProgress } from "../../types";
import { formatNumber } from "../../utils/format";
import { Button, ProgressBar } from "../ui";

export function ScanProgressPanel({ progress, onCancel, cancelling }: { progress: ScanProgress | null; onCancel: () => void; cancelling: boolean }) {
  const analysing = progress?.phase === "analysing";
  const frac = analysing && progress.total > 0 ? progress.processed / progress.total : undefined;
  return (
    <div className="grid gap-3 rounded-lg border border-line bg-panel p-4" aria-live="polite">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2 text-[13px] font-medium">
          <Loader2 size={15} className="animate-spin text-accent" />
          {cancelling ? "Cancelling…" : analysing ? "Analysing samples (reading headers, hashing, classifying)" : "Scanning folders"}
        </div>
        <Button size="sm" variant="ghost" onClick={onCancel} disabled={cancelling}>
          Cancel
        </Button>
      </div>
      <ProgressBar value={frac} indeterminate={!analysing} />
      <div className="grid grid-cols-4 gap-3 text-xs">
        <div>
          <div className="text-faint">Files found</div>
          <div className="tabular text-fg">{formatNumber(progress?.discovered ?? 0)}</div>
        </div>
        <div>
          <div className="text-faint">Supported</div>
          <div className="tabular text-fg">{formatNumber(analysing ? progress.total : (progress?.supported ?? 0))}</div>
        </div>
        <div>
          <div className="text-faint">Unsupported</div>
          <div className="tabular text-fg">{formatNumber(progress?.unsupported ?? 0)}</div>
        </div>
        <div>
          <div className="text-faint">Analysed</div>
          <div className="tabular text-fg">{analysing ? `${formatNumber(progress.processed)} / ${formatNumber(progress.total)}` : "—"}</div>
        </div>
      </div>
      <div className="truncate font-mono text-[11px] text-faint" title={progress?.current}>
        {progress?.current || " "}
      </div>
    </div>
  );
}
