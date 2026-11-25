import { Check, Download, FolderOpen, ListFilter } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useApp } from "../../hooks/useApp";
import { api, errorMessage } from "../../services/api";
import { pickTextSavePath } from "../../services/dialogs";
import type { BulkResult, ReviewGroup, ReviewOverview, SampleQuery } from "../../types";
import { formatNumber, percent } from "../../utils/format";
import { Badge, Button, EmptyState, ProgressBar } from "../ui";
import { CategoryPicker } from "./CategoryPicker";

interface Props {
  importId: string;
  threshold: number;
  editable: boolean;
  version: number;
  onShowFiles: (g: ReviewGroup) => void;
  onChanged: () => void;
}

/** The review queue grouped by folder and prediction, so thousands of
 * files can be approved or corrected a group at a time. */
export function ReviewGroups({ importId, threshold, editable, version, onShowFiles, onChanged }: Props) {
  const { toast, config } = useApp();
  const [ov, setOv] = useState<ReviewOverview | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    api
      .reviewOverview(importId)
      .then(setOv)
      .catch((e) => toast(errorMessage(e), "error"));
  }, [importId, toast]);
  useEffect(load, [load, version]);

  const report = (verb: string, r: BulkResult) => {
    const auto = r.resolvedAutomatically > 0 ? ` ${formatNumber(r.resolvedAutomatically)} similar file(s) were resolved automatically.` : "";
    toast(`${verb} ${formatNumber(r.changed)} file(s).${auto} ${formatNumber(r.remaining)} left to review.`, "ok");
  };

  const run = async (fn: () => Promise<BulkResult>, verb: string) => {
    setBusy(true);
    try {
      report(verb, await fn());
      onChanged();
      load();
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      setBusy(false);
    }
  };

  const groupQuery = (g: ReviewGroup): SampleQuery => ({
    importId,
    needsReviewBelow: threshold,
    relativeDir: g.folder,
    categoryId: g.categoryId,
  });

  const exportList = async () => {
    const path = await pickTextSavePath("kit-organizer-review-list.txt");
    if (!path) return;
    try {
      const n = await api.exportReviewList(importId, path);
      toast(`Exported ${formatNumber(n)} file names. Only names are included, no audio.`, "ok");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  if (!ov) return <div className="p-6"><ProgressBar indeterminate /></div>;
  if (ov.total === 0) {
    return (
      <EmptyState icon={<Check size={18} />} title="Nothing left to review">
        Every file is above the confidence threshold or has been reviewed.
      </EmptyState>
    );
  }
  const unc = config?.uncategorisedCategory ?? "uncategorised";
  const maxReason = Math.max(...ov.reasons.map(([, n]) => n), 1);

  return (
    <div className="grid content-start gap-4 p-5">
      <section className="grid gap-4 rounded-lg border border-line bg-panel p-4 lg:grid-cols-[1fr_1fr]">
        <div className="grid content-start gap-3">
          <div>
            <div className="tabular text-2xl font-semibold">{formatNumber(ov.total)}</div>
            <div className="text-xs text-muted">files need review, in {formatNumber(ov.groups.length)}{ov.groups.length === 300 ? "+" : ""} groups</div>
          </div>
          <div className="flex flex-wrap gap-1.5">
            <Badge tone="warn">{formatNumber(ov.nearThreshold)} close to the threshold</Badge>
            <Badge>{formatNumber(ov.low)} low confidence</Badge>
            <Badge>{formatNumber(ov.uncategorised)} uncategorised</Badge>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="primary"
              icon={<Check size={13} />}
              disabled={!editable || busy || ov.nearThreshold === 0}
              onClick={() => void run(() => api.acceptMatching({ importId, needsReviewBelow: threshold, minConfidence: Math.max(0, threshold - 0.15) }), "Accepted")}
              title={`Accept every prediction between ${percent(Math.max(0, threshold - 0.15))} and ${percent(threshold)}`}
            >
              Accept all close calls ({formatNumber(ov.nearThreshold)})
            </Button>
            <Button size="sm" icon={<Download size={13} />} onClick={() => void exportList()}>
              Export list for tuning
            </Button>
          </div>
          <p className="text-[11px] text-faint">
            Every decision is learned: after you accept or move a group, the rest of the import is re-classified and similar files drop out of the queue.
          </p>
        </div>
        <div>
          <div className="mb-2 text-xs font-medium text-muted">Why they need review</div>
          <ul className="grid gap-1.5">
            {ov.reasons.map(([r, n]) => (
              <li key={r} className="grid grid-cols-[1fr_auto] items-center gap-x-3 text-xs">
                <span className="truncate">{r}</span>
                <span className="tabular text-muted">{formatNumber(n)}</span>
                <div className="col-span-2 h-1 overflow-hidden rounded bg-raised">
                  <div className="h-full bg-accent/70" style={{ width: `${(n / maxReason) * 100}%` }} />
                </div>
              </li>
            ))}
          </ul>
        </div>
      </section>

      <ul className="divide-y divide-line rounded-lg border border-line bg-panel" aria-label="Review groups">
        {ov.groups.map((g) => (
          <li key={`${g.folder}|${g.categoryId}`} className="grid grid-cols-[1fr_auto] items-center gap-3 px-4 py-2.5">
            <div className="min-w-0">
              <div className="flex items-center gap-2 text-[13px]">
                <FolderOpen size={13} className="shrink-0 text-faint" />
                <span className="truncate text-muted" title={g.folder}>{g.folder}</span>
                <span className="shrink-0 text-faint">→</span>
                <span className={g.categoryId === unc ? "shrink-0 text-warn" : "shrink-0 text-fg"}>{g.categoryPath}</span>
                <Badge className="shrink-0">{formatNumber(g.count)}</Badge>
                {g.categoryId !== unc && <span className="tabular shrink-0 text-[11px] text-faint">avg {percent(g.avgConfidence)}</span>}
              </div>
              <div className="mt-0.5 truncate text-[11px] text-faint" title={g.examples.join(", ")}>
                {g.reason} · {g.examples.join(", ")}
                {g.count > g.examples.length ? ", …" : ""}
              </div>
            </div>
            <div className="flex items-center gap-1.5">
              <Button
                size="sm"
                icon={<Check size={13} />}
                disabled={!editable || busy}
                title={g.categoryId === unc ? "Leave these in Uncategorised and remove them from the review queue" : `Accept ${g.categoryPath} for all ${g.count} files`}
                onClick={() => void run(() => api.acceptMatching(groupQuery(g)), g.categoryId === unc ? "Kept" : "Accepted")}
              >
                {g.categoryId === unc ? "Keep" : "Accept"}
              </Button>
              <CategoryPicker
                label="Move to…"
                align="right"
                disabled={!editable || busy}
                onSelect={(id) => void run(() => api.reassignMatching(groupQuery(g), id), "Moved")}
              />
              <Button size="sm" variant="ghost" icon={<ListFilter size={13} />} onClick={() => onShowFiles(g)}>
                Files
              </Button>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
