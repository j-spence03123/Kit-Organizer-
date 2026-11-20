import { AlertCircle, Copy, FolderInput, FolderOutput, Layers, Library, ListChecks } from "lucide-react";
import { useEffect, useState } from "react";
import { ImportStatusBadge } from "../components/import/ImportStatusBadge";
import { PageHeader } from "../components/layout/PageHeader";
import { Button, EmptyState, Panel, Stat } from "../components/ui";
import { useApp } from "../hooks/useApp";
import { api } from "../services/api";
import type { DashboardStats, ImportRecord } from "../types";
import { basename, formatDate, formatNumber } from "../utils/format";

export default function Dashboard() {
  const { navigate, setActiveImportId, dataVersion } = useApp();
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [recent, setRecent] = useState<ImportRecord[]>([]);

  useEffect(() => {
    api.dashboardStats().then(setStats).catch(() => undefined);
    api.listImports(8).then(setRecent).catch(() => undefined);
  }, [dataVersion]);

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Dashboard"
        subtitle="Your local sample library at a glance."
        actions={
          <Button variant="primary" icon={<FolderInput size={14} />} onClick={() => navigate("import")}>
            Import samples
          </Button>
        }
      />
      <div className="min-h-0 flex-1 overflow-auto px-6 py-5">
        <div className="mx-auto grid max-w-6xl gap-5">
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <Stat label="Total imported samples" value={formatNumber(stats?.totalImported ?? 0)} icon={<Layers size={14} />} sub={`${formatNumber(stats?.imports ?? 0)} imports`} />
            <Stat label="Samples organised" value={formatNumber(stats?.organised ?? 0)} icon={<Library size={14} />} />
            <Stat label="Awaiting review" value={formatNumber(stats?.awaitingReview ?? 0)} icon={<ListChecks size={14} />} sub="Below the confidence threshold" />
            <Stat label="Duplicates detected" value={formatNumber(stats?.duplicates ?? 0)} icon={<Copy size={14} />} sub="Identical file contents" />
          </div>

          <div className="grid gap-5 lg:grid-cols-[1fr_320px]">
            <Panel title="Recent imports" actions={<Button size="sm" variant="ghost" onClick={() => navigate("history")}>View all</Button>}>
              {recent.length === 0 ? (
                <EmptyState icon={<FolderInput size={18} />} title="No imports yet">
                  Drag a sample pack folder onto the Import page to get started.
                </EmptyState>
              ) : (
                <ul className="divide-y divide-line">
                  {recent.map((r) => (
                    <li key={r.id}>
                      <button
                        type="button"
                        className="flex w-full items-center gap-3 px-4 py-2.5 text-left hover:bg-panel-2"
                        onClick={() => {
                          setActiveImportId(r.id);
                          navigate(["review", "cancelled", "interrupted"].includes(r.status) ? "review" : "history");
                        }}
                      >
                        <div className="min-w-0 flex-1">
                          <div className="truncate text-[13px]">{r.sources.map(basename).join(", ")}</div>
                          <div className="text-xs text-faint">
                            {formatDate(r.createdAt)} · {formatNumber(r.stats.supported)} samples
                            {r.stats.copied + r.stats.moved > 0 && ` · ${formatNumber(r.stats.copied + r.stats.moved)} organised`}
                          </div>
                        </div>
                        <ImportStatusBadge status={r.status} />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </Panel>
            <div className="grid content-start gap-5">
              <Panel title="Last output folder">
                <div className="flex items-start gap-2.5 p-4">
                  <FolderOutput size={16} className="mt-0.5 shrink-0 text-accent" />
                  <span className="break-all font-mono text-[11px] text-muted" data-selectable>{stats?.lastOutputDir || "None yet"}</span>
                </div>
              </Panel>
              <Panel title="How classification works">
                <div className="grid gap-2 p-4 text-xs text-muted">
                  <p>Kit Organizer reads folder names, filenames and audio metadata (duration, ACID loop/tempo tags). It does not listen to the audio itself.</p>
                  <p className="flex gap-2">
                    <AlertCircle size={13} className="mt-px shrink-0 text-accent" />
                    Confidence reflects how strong and consistent the rule evidence is, not a statistical probability.
                  </p>
                </div>
              </Panel>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
