import type { ImportStatus } from "../../types";
import { Badge } from "../ui";

const MAP: Record<ImportStatus, { label: string; tone: "neutral" | "accent" | "ok" | "warn" | "bad" }> = {
  scanning: { label: "Scanning", tone: "accent" },
  review: { label: "Awaiting review", tone: "accent" },
  organising: { label: "Organising", tone: "accent" },
  completed: { label: "Completed", tone: "ok" },
  cancelled: { label: "Cancelled", tone: "warn" },
  interrupted: { label: "Interrupted", tone: "warn" },
  undone: { label: "Undone", tone: "neutral" },
  "partially-undone": { label: "Partially undone", tone: "warn" },
  failed: { label: "Failed", tone: "bad" },
};

export function ImportStatusBadge({ status }: { status: ImportStatus }) {
  const m = MAP[status];
  return <Badge tone={m.tone}>{m.label}</Badge>;
}
