import { Check, CopyCheck, CopyX, FolderInput, ListChecks, Package, RefreshCw, Search, Sparkles, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CategoryPicker } from "../components/classification/CategoryPicker";
import { OrganiseDialog } from "../components/classification/OrganiseDialog";
import { ReviewGroups } from "../components/classification/ReviewGroups";
import { SampleDetail } from "../components/classification/SampleDetail";
import { Pager, SampleTable } from "../components/classification/SampleTable";
import { PageHeader } from "../components/layout/PageHeader";
import { Badge, Button, EmptyState, IconButton, Modal, inputBase, inputClass } from "../components/ui";
import { useApp } from "../hooks/useApp";
import { usePlayer } from "../hooks/useAudioPlayer";
import { useDebounced, useSampleQuery } from "../hooks/useSampleQuery";
import { api, errorMessage } from "../services/api";
import type { BulkResult, ImportRecord, ImportSummary, ReviewGroup, SampleQuery, SampleRow, SimilarSamples } from "../types";
import { basename, cx, formatDate, formatNumber } from "../utils/format";

const PAGE = 200;
type Filter = "all" | "review" | "duplicates" | "high" | "manual";

export default function Review() {
  const { activeImportId, setActiveImportId, settings, config, categories, toast, navigate, bumpData, dataVersion } = useApp();
  const threshold = settings?.confidenceThreshold ?? 0.6;
  const budget = settings?.reviewBudget ?? 0;
  const [imp, setImp] = useState<ImportRecord | null>(null);
  const [candidates, setCandidates] = useState<ImportRecord[]>([]);
  const [summary, setSummary] = useState<ImportSummary | null>(null);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [category, setCategory] = useState("");
  const [sort, setSort] = useState("folder");
  const [descending, setDescending] = useState(false);
  const [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [anchor, setAnchor] = useState<string | null>(null);
  const [focused, setFocused] = useState<SampleRow | null>(null);
  const [version, setVersion] = useState(0);
  const [organiseOpen, setOrganiseOpen] = useState(false);
  const [packDialog, setPackDialog] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [similar, setSimilar] = useState<SimilarSamples | null>(null);
  const [view, setView] = useState<"groups" | "files">("groups");
  const [folderFilter, setFolderFilter] = useState<ReviewGroup | null>(null);
  const [allMatching, setAllMatching] = useState(false);
  const debounced = useDebounced(search);
  const player = usePlayer();
  // Optional "play on select" preference.
  const focusRow = (r: SampleRow) => {
    setFocused(r);
    if (settings?.preview.autoplay && r.fileKind === "audio") {
      void player.play(r.status === "organised" && r.destinationPath ? r.destinationPath : r.originalPath);
    }
  };

  // Resolve which import to review.
  useEffect(() => {
    if (activeImportId) {
      api.getImport(activeImportId).then(setImp).catch(() => setImp(null));
    } else {
      setImp(null);
      api
        .listImports(50)
        .then((l) => setCandidates(l.filter((i) => ["review", "cancelled", "interrupted"].includes(i.status))))
        .catch(() => undefined);
    }
  }, [activeImportId, version, dataVersion]);

  useEffect(() => {
    if (activeImportId) api.importSummary(activeImportId).then(setSummary).catch(() => undefined);
  }, [activeImportId, version]);
  // Short queues are easiest to work through as a plain list — decided once
  // when an import opens, so it never overrides a view the user picked.
  const viewChosenFor = useRef<string | null>(null);
  useEffect(() => {
    if (summary && activeImportId && viewChosenFor.current !== activeImportId) {
      viewChosenFor.current = activeImportId;
      if (summary.needsReview <= 100) setView("files");
    }
  }, [summary, activeImportId]);

  useEffect(() => setOffset(0), [debounced, filter, category, sort, descending, folderFilter]);
  useEffect(() => setAllMatching(false), [debounced, filter, category, folderFilter, activeImportId]);

  const query: SampleQuery | null = useMemo(
    () =>
      activeImportId
        ? {
            importId: activeImportId,
            search: debounced || null,
            category: category || null,
            needsReviewBelow: filter === "review" ? threshold : null,
            duplicatesOnly: filter === "duplicates",
            minConfidence: filter === "high" ? 0.8 : null,
            relativeDir: folderFilter && !folderFilter.includesSubfolders ? folderFilter.folder : null,
            relativeDirPrefix: folderFilter?.includesSubfolders ? folderFilter.folder : null,
            categoryId: folderFilter?.categoryId ?? null,
            sort,
            descending,
            offset,
            limit: PAGE,
          }
        : null,
    [activeImportId, debounced, category, filter, threshold, sort, descending, offset, folderFilter],
  );
  const { page, loading } = useSampleQuery(query, version);
  const rows = filter === "manual" ? page.rows.filter((r) => r.manualOverride) : page.rows;

  // Keep the focused row fresh after edits.
  useEffect(() => {
    if (focused) {
      const fresh = page.rows.find((r) => r.id === focused.id);
      if (fresh && fresh !== focused) setFocused(fresh);
    }
  }, [page.rows, focused]);

  const refresh = useCallback(() => setVersion((v) => v + 1), []);

  const run = async (fn: () => Promise<unknown>, ok?: string) => {
    setBusy(true);
    try {
      await fn();
      if (ok) toast(ok, "ok");
      refresh();
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      setBusy(false);
    }
  };

  const ids = [...selected];
  // Bulk actions over every file matching the current filters (not just this page).
  const matchingQuery = (): SampleQuery => ({ ...query, offset: 0, limit: 0 });
  const bulkReport = (verb: string, r: BulkResult) => {
    const auto = r.resolvedAutomatically > 0 ? ` ${formatNumber(r.resolvedAutomatically)} similar file(s) resolved automatically.` : "";
    toast(`${verb} ${formatNumber(r.changed)} file(s).${auto} ${formatNumber(r.remaining)} left to review.`, "ok");
  };
  const runBulk = async (fn: () => Promise<BulkResult>, verb: string) => {
    setBusy(true);
    try {
      bulkReport(verb, await fn());
      setSelected(new Set());
      setAllMatching(false);
      refresh();
      bumpData();
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      setBusy(false);
    }
  };
  const catName = (id: string) => categories.find((c) => c.id === id)?.path ?? id;
  const reassign = async (list: string[], id: string) => {
    await run(() => api.reassignSamples(list, id), `Moved ${list.length} sample(s) to ${catName(id)}`);
    // Offer to fix files that follow the same naming pattern.
    try {
      setSimilar(await api.similarSamples(list));
    } catch {
      setSimilar(null);
    }
  };
  const applySimilar = async () => {
    if (!similar) return;
    const s = similar;
    setSimilar(null);
    await run(() => api.reassignSamples(s.ids, s.categoryId), `Moved ${s.ids.length} similar sample(s) to ${s.categoryPath}`);
  };
  const accept = (list: string[]) => run(() => api.markReviewed(list, true), `Accepted ${list.length} prediction(s)`);
  const dup = (list: string[], d: "keep" | "skip") => run(() => api.setDuplicateDecision(list, d));
  const pack = (list: string[], name: string) => run(() => api.setSourcePack(list, name), "Source pack updated");

  const toggle = (r: SampleRow, shift: boolean) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (shift && anchor) {
        const a = rows.findIndex((x) => x.id === anchor);
        const b = rows.findIndex((x) => x.id === r.id);
        if (a >= 0 && b >= 0) {
          for (const x of rows.slice(Math.min(a, b), Math.max(a, b) + 1)) next.add(x.id);
          return next;
        }
      }
      if (next.has(r.id)) next.delete(r.id);
      else next.add(r.id);
      return next;
    });
    setAnchor(r.id);
  };

  if (!activeImportId || !imp) {
    return (
      <div className="flex h-full flex-col">
        <PageHeader title="Review" subtitle="Check classifications before any files are changed." />
        {candidates.length === 0 ? (
          <EmptyState icon={<ListChecks size={20} />} title="Nothing to review" action={<Button variant="primary" icon={<FolderInput size={14} />} onClick={() => navigate("import")}>Import samples</Button>}>
            Scan some sample packs first. Their proposed destinations will appear here.
          </EmptyState>
        ) : (
          <div className="mx-auto mt-6 w-full max-w-2xl px-6">
            <h2 className="mb-2 text-xs font-medium text-muted">Imports waiting for review</h2>
            <ul className="divide-y divide-line rounded-lg border border-line bg-panel">
              {candidates.map((c) => (
                <li key={c.id}>
                  <button type="button" className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left hover:bg-panel-2" onClick={() => setActiveImportId(c.id)}>
                    <span className="min-w-0">
                      <span className="block truncate text-[13px]">{c.sources.map(basename).join(", ")}</span>
                      <span className="text-xs text-faint">{formatDate(c.createdAt)} · {formatNumber(c.stats.supported)} samples</span>
                    </span>
                    <Badge tone={c.status === "review" ? "accent" : "warn"}>{c.status}</Badge>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    );
  }

  const editable = ["review", "cancelled", "interrupted", "completed", "partially-undone"].includes(imp.status) && (summary?.pending ?? 0) > 0;
  const showGroups = filter === "review" && view === "groups" && !folderFilter;
  const pageAllSelected = rows.length > 0 && rows.every((r) => selected.has(r.id));
  const filters: [Filter, string, number | undefined][] = [
    ["all", "All", summary?.total],
    ["review", "Needs review", summary?.needsReview],
    ["duplicates", "Duplicates", summary?.duplicates],
    ["high", "High confidence", undefined],
    ["manual", "Manual (this page)", undefined],
  ];

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Review"
        subtitle={
          <>
            {imp.sources.map(basename).join(", ")} · {formatDate(imp.createdAt)} · {imp.mode === "move" ? "Move" : "Copy"} mode
          </>
        }
        actions={
          <>
            <Button
              size="sm"
              variant="ghost"
              icon={<RefreshCw size={13} />}
              disabled={busy || !editable}
              onClick={() => run(() => api.reclassifyImport(imp.id), "Re-ran classification with the current categories (manual changes kept)")}
            >
              Re-classify
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setActiveImportId(null)}>
              Switch import
            </Button>
            <Button variant="primary" icon={<Sparkles size={14} />} disabled={!editable} onClick={() => setOrganiseOpen(true)}>
              Organise Samples
            </Button>
          </>
        }
      />

      <div className="flex flex-wrap items-center gap-2 border-b border-line px-6 py-2.5">
        <div className="relative">
          <Search size={13} className="absolute left-2.5 top-2.5 text-faint" />
          <input className={cx(inputBase, "w-64 pl-8")} placeholder="Search filename, folder or pack" value={search} onChange={(e) => setSearch(e.target.value)} aria-label="Search samples" />
        </div>
        <div className="flex rounded-md border border-line-strong bg-panel-2 p-0.5" role="tablist" aria-label="Filter">
          {filters.map(([k, l, n]) => (
            <button key={k} type="button" role="tab" aria-selected={filter === k} onClick={() => { setFilter(k); setFolderFilter(null); if (k === "review") setView("groups"); }} className={cx("rounded px-2.5 py-1 text-xs", filter === k ? "bg-raised text-fg" : "text-muted hover:text-fg")}>
              {l}
              {n != null && <span className={cx("tabular ml-1.5", k === "review" && n > 0 ? "text-warn" : "text-faint")}>{formatNumber(n)}</span>}
            </button>
          ))}
        </div>
        <select className={cx(inputBase, "w-56")} value={category} onChange={(e) => setCategory(e.target.value)} aria-label="Filter by category">
          <option value="">All categories</option>
          {categories.map((c) => (
            <option key={c.id} value={c.path}>
              {"  ".repeat(c.depth)}
              {c.name}
            </option>
          ))}
        </select>
        {filter === "review" && !folderFilter && (
          <div className="flex rounded-md border border-line-strong bg-panel-2 p-0.5" role="radiogroup" aria-label="Review view">
            {(["groups", "files"] as const).map((v) => (
              <button key={v} type="button" role="radio" aria-checked={view === v} onClick={() => setView(v)} className={cx("rounded px-2.5 py-1 text-xs capitalize", view === v ? "bg-raised text-fg" : "text-muted hover:text-fg")}>
                {v}
              </button>
            ))}
          </div>
        )}
        {folderFilter && (
          <span className="inline-flex items-center gap-1.5 rounded-md border border-accent/40 bg-accent-soft px-2 py-1 text-xs">
            <span className="max-w-72 truncate" title={folderFilter.folder}>{folderFilter.folder}</span>
            <span className="text-faint">→ {folderFilter.categoryPath}</span>
            <button type="button" aria-label="Clear folder filter" className="text-faint hover:text-fg" onClick={() => { setFolderFilter(null); setView("groups"); }}>
              <X size={12} />
            </button>
          </span>
        )}
        {!showGroups && (
          <div className="ml-auto">
            <Pager offset={offset} limit={PAGE} total={page.total} onChange={setOffset} />
          </div>
        )}
      </div>

      {selected.size > 0 && (
        <div className="flex items-center gap-2 border-b border-line bg-accent-soft px-6 py-2" role="toolbar" aria-label="Bulk actions">
          <span className="text-xs font-medium">{allMatching ? `All ${formatNumber(page.total)} matching selected` : `${selected.size} selected`}</span>
          {pageAllSelected && page.total > rows.length && !allMatching && (
            <button type="button" className="text-xs text-accent underline-offset-2 hover:underline" onClick={() => setAllMatching(true)}>
              Select all {formatNumber(page.total)} matching
            </button>
          )}
          <CategoryPicker
            onSelect={(id) => void (allMatching ? runBulk(() => api.reassignMatching(matchingQuery(), id), "Moved") : reassign(ids, id))}
            disabled={!editable || busy}
            label="Reassign to…"
          />
          <Button
            size="sm"
            icon={<Check size={13} />}
            disabled={!editable || busy}
            onClick={() => void (allMatching ? runBulk(() => api.acceptMatching(matchingQuery()), "Accepted") : accept(ids))}
          >
            Accept predictions
          </Button>
          <Button size="sm" icon={<CopyX size={13} />} disabled={!editable || busy || allMatching} onClick={() => void dup(ids, "skip")}>
            Skip duplicates
          </Button>
          <Button size="sm" icon={<CopyCheck size={13} />} disabled={!editable || busy || allMatching} onClick={() => void dup(ids, "keep")}>
            Keep duplicates
          </Button>
          <Button size="sm" icon={<Package size={13} />} disabled={busy || allMatching} onClick={() => setPackDialog(focused?.sourcePack ?? "")}>
            Set source pack
          </Button>
          <IconButton label="Clear selection" onClick={() => setSelected(new Set())} className="ml-auto">
            <X size={14} />
          </IconButton>
        </div>
      )}

      {summary && editable && ((summary.autoResolved > 0) || (budget > 0 && summary.needsReview > budget)) && (
        <div className="flex flex-wrap items-center gap-3 border-b border-line bg-panel-2 px-6 py-2 text-xs" role="status" aria-label="Automatic resolution">
          <Sparkles size={14} className="shrink-0 text-accent" />
          {summary.autoResolved > 0 ? (
            <span className="min-w-0 flex-1">
              <strong>{formatNumber(summary.autoResolved)}</strong> files were resolved automatically: best guesses of {Math.round((settings?.autoAcceptFloor ?? 0.35) * 100)}%+ were accepted, the rest kept in Uncategorised with their original folders. Each file left to review decides for its whole group (×N).
            </span>
          ) : (
            <span className="min-w-0 flex-1">
              {formatNumber(summary.needsReview)} files need review. Resolve the rest automatically and keep only the {budget} decisions that matter most?
            </span>
          )}
          {summary.autoResolved > 0 ? (
            <>
              <Button size="sm" disabled={busy} onClick={() => run(() => api.autoResolve(imp.id, true), "Brought the next decisions forward")}>
                Review {budget} more
              </Button>
              <Button size="sm" variant="ghost" disabled={busy} onClick={() => run(() => api.undoAutoResolve(imp.id), "Automatic resolution undone — every uncertain file is back in review")}>
                Undo auto-resolve
              </Button>
            </>
          ) : (
            <Button size="sm" variant="primary" disabled={busy} onClick={() => run(() => api.autoResolve(imp.id, false), `Review queue reduced to ${budget}`)}>
              Reduce to {budget}
            </Button>
          )}
        </div>
      )}

      {similar && (
        <div className="flex items-center gap-3 border-b border-line bg-panel-2 px-6 py-2 text-xs" role="status" aria-label="Similar files">
          <Sparkles size={14} className="shrink-0 text-accent" />
          <span className="min-w-0 flex-1">
            Found <strong>{formatNumber(similar.ids.length)}</strong> more {similar.description}. Move them to <strong>{similar.categoryPath}</strong> too?
          </span>
          <Button size="sm" variant="primary" disabled={busy} onClick={() => void applySimilar()}>
            Apply to {formatNumber(similar.ids.length)}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setSimilar(null)}>
            Dismiss
          </Button>
        </div>
      )}

      {showGroups ? (
        <div className="min-h-0 flex-1 overflow-auto">
          <ReviewGroups
            importId={imp.id}
            threshold={threshold}
            editable={editable}
            version={version}
            onShowFiles={(g) => {
              setFolderFilter(g);
              setView("files");
            }}
            onChanged={() => {
              refresh();
              bumpData();
            }}
          />
        </div>
      ) : (
      <div className="flex min-h-0 flex-1">
        <div className={cx("min-w-0 flex-1 overflow-auto", loading && "opacity-70")}>
          <SampleTable
            rows={rows}
            columns={["filename", "folder", "category", "confidence", "destination", "status"]}
            threshold={threshold}
            rootFolder={config?.rootFolder ?? "Organised Samples"}
            selected={selected}
            focusedId={focused?.id ?? null}
            onFocus={focusRow}
              onSelect={setFocused}
            onToggle={toggle}
            onToggleAll={() => setSelected((s) => (rows.every((r) => s.has(r.id)) ? new Set() : new Set(rows.map((r) => r.id))))}
            sort={sort}
            descending={descending}
            onSort={(k) => {
              if (k === sort) setDescending(!descending);
              else {
                setSort(k);
                setDescending(k === "confidence");
              }
            }}
            compact={settings?.appearance.compactRows}
            empty={<p className="px-6 py-10 text-center text-xs text-muted">No samples match these filters.</p>}
          />
        </div>
        <aside className="w-[330px] shrink-0 overflow-auto border-l border-line bg-panel" aria-label="Sample details">
          {focused ? (
            <SampleDetail
              key={focused.id}
              row={focused}
              threshold={threshold}
              editable={editable && focused.status === "pending"}
              onReassign={(l, id) => void reassign(l, id)}
              onAccept={(l) => void accept(l)}
              onDuplicate={(l, d) => void dup(l, d)}
              onSourcePack={(l, p) => void pack(l, p)}
            />
          ) : (
            <EmptyState icon={<ListChecks size={18} />} title="Select a sample">
              Click a row to preview it and see why it was classified that way. Ctrl/⌘- or Shift-click to select several.
            </EmptyState>
          )}
        </aside>
      </div>
      )}

      {organiseOpen && (
        <OrganiseDialog
          open={organiseOpen}
          imp={imp}
          onClose={() => setOrganiseOpen(false)}
          onFinished={() => {
            refresh();
            bumpData();
          }}
        />
      )}
      <Modal
        open={packDialog !== null}
        title={`Set source pack for ${selected.size} sample(s)`}
        onClose={() => setPackDialog(null)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setPackDialog(null)}>Cancel</Button>
            <Button
              variant="primary"
              disabled={!packDialog?.trim()}
              onClick={() => {
                void pack(ids, packDialog ?? "");
                setPackDialog(null);
              }}
            >
              Save
            </Button>
          </>
        }
      >
        <input className={inputClass} value={packDialog ?? ""} onChange={(e) => setPackDialog(e.target.value)} aria-label="Source pack name" autoFocus />
        <p className="mt-2 text-xs text-muted">Provenance is kept even after files are reorganised.</p>
      </Modal>
    </div>
  );
}
