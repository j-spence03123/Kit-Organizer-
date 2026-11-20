import type { SampleRow } from "../../types";
import { Badge } from "../ui";

export function sampleState(r: SampleRow, threshold: number) {
  if (r.status === "organised") return { label: "Organised", tone: "ok" as const };
  if (r.status === "skipped") return { label: "Skipped", tone: "neutral" as const };
  if (r.status === "failed") return { label: "Error", tone: "bad" as const };
  if (r.status === "undone") return { label: "Undone", tone: "neutral" as const };
  if (r.duplicateOf) {
    if (!r.dupDecision) return { label: "Duplicate?", tone: "warn" as const };
    return r.dupDecision === "skip" ? { label: "Duplicate · skip", tone: "neutral" as const } : { label: "Duplicate · keep", tone: "accent" as const };
  }
  if (r.manualOverride) return { label: "Manual", tone: "accent" as const };
  if (!r.reviewed && r.confidence < threshold) return { label: "Needs review", tone: "warn" as const };
  return { label: r.reviewed ? "Reviewed" : "Ready", tone: "ok" as const };
}

export function StatusBadge({ row, threshold }: { row: SampleRow; threshold: number }) {
  const s = sampleState(row, threshold);
  return <Badge tone={s.tone}>{s.label}</Badge>;
}
