import { ChevronDown, ChevronRight, History as HistoryIcon, ListChecks, Trash2, Undo2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { ImportStatusBadge } from "../components/import/ImportStatusBadge";
import { PageHeader } from "../components/layout/PageHeader";
import { Badge, Button, EmptyState, Modal } from "../components/ui";
import { useApp } from "../hooks/useApp";
import { api, errorMessage } from "../services/api";
import type { ImportRecord, OperationRow, UndoReport } from "../types";
import { basename, cx, formatDate, formatNumber } from "../utils/format";

const UNDOABLE = ["completed", "cancelled", "interrupted", "partially-undone"];

function OperationsList({ importId }: { importId: string }) {
  const [ops, setOps] = useState<OperationRow[] | null>(null);
  useEffect(() => {
    api.importOperations(importId).then(setOps).catch(() => setOps([]));
  }, [importId]);
  if (!ops) return <p className="px-4 py-3 text-xs text-faint">Loading…</p>;
  if (ops.length === 0) return <p className="px-4 py-3 text-xs text-faint">No file operations were performed.</p>;
  return (
    <div className="max-h-72 overflow-auto border-t border-line" data-selectable>
      <table className="w-full text-[11px]">
        <thead className="sticky top-0 bg-panel text-left text-faint">
          <tr>
            <th className="px-4 py-1.5 font-medium">Operation</th>
            <th className="px-2 py-1.5 font-medium">From</th>
            <th className="px-2 py-1.5 font-medium">To</th>
            <th className="px-2 py-1.5 font-medium">State</th>
          </tr>
        </thead>
        <tbody>
          {ops.map((o) => (
            <tr key={o.id} className="border-t border-line/60 align-top">
              <td className="px-4 py-1.5 text-muted">{o.op}</td>
              <td className="max-w-xs truncate px-2 py-1.5 font-mono text-muted" title={o.src}>{o.src}</td>
              <td className="max-w-xs truncate px-2 py-1.5 font-mono text-muted" title={o.dest}>{o.dest}</td>
              <td className="px-2 py-1.5">
                <span className={cx(o.state === "done" ? "text-ok" : o.state === "failed" ? "text-bad" : "text-muted")}>{o.state}</span>
                {o.error && <div className="text-warn">{o.error}</div>}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export default function History() {
  const { toast, setActiveImportId, navigate, bumpData, dataVersion } = useApp();
  const [imports, setImports] = useState<ImportRecord[]>([]);
  const [open, setOpen] = useState<string | null>(null);
  const [confirmUndo, setConfirmUndo] = useState<ImportRecord | null>(null);
  const [undoResult, setUndoResult] = useState<UndoReport | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    api.listImports(200).then(setImports).catch((e) => toast(errorMessage(e), "error"));
  }, [toast]);
  useEffect(load, [load, dataVersion]);

  const undo = async (imp: ImportRecord) => {
    setBusy(true);
    try {
      const r = await api.undoImport(imp.id);
      setUndoResult(r);
      setConfirmUndo(null);
      bumpData();
      load();
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      setBusy(false);
    }
  };

  const remove = async (imp: ImportRecord) => {
    try {
      await api.deleteImport(imp.id);
      toast("Import removed from history", "ok");
      bumpData();
      load();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  return (
    <div className="flex h-full flex-col">
      <PageHeader title="History" subtitle="Every scan and organisation run, with safe undo." />
      <div className="min-h-0 flex-1 overflow-auto px-6 py-5">
        {imports.length === 0 ? (
          <EmptyState icon={<HistoryIcon size={20} />} title="No history yet" />
        ) : (
          <ul className="mx-auto grid max-w-5xl gap-2">
            {imports.map((imp) => {
              const organised = imp.stats.copied + imp.stats.moved - imp.stats.reverted;
              const expanded = open === imp.id;
              return (
                <li key={imp.id} className="rounded-lg border border-line bg-panel">
                  <div className="flex items-center gap-3 px-4 py-3">
                    <button type="button" aria-label={expanded ? "Collapse" : "Expand"} aria-expanded={expanded} onClick={() => setOpen(expanded ? null : imp.id)} className="text-faint hover:text-fg">
                      {expanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
                    </button>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate text-[13px] font-medium">{imp.sources.map(basename).join(", ")}</span>
                        <ImportStatusBadge status={imp.status} />
                        <Badge>{imp.mode}</Badge>
                      </div>
                      <div className="mt-0.5 text-xs text-faint">
                        {formatDate(imp.createdAt)} · {formatNumber(imp.stats.supported)} samples · {formatNumber(imp.stats.copied)} copied · {formatNumber(imp.stats.moved)} moved ·{" "}
                        {formatNumber(imp.stats.skipped)} skipped · {formatNumber(imp.stats.duplicates)} duplicates · {formatNumber(imp.stats.errors)} errors
                      </div>
                    </div>
                    {["review", "cancelled", "interrupted"].includes(imp.status) && (
                      <Button
                        size="sm"
                        icon={<ListChecks size={13} />}
                        onClick={() => {
                          setActiveImportId(imp.id);
                          navigate("review");
                        }}
                      >
                        Review
                      </Button>
                    )}
                    {UNDOABLE.includes(imp.status) && organised > 0 && (
                      <Button size="sm" icon={<Undo2 size={13} />} onClick={() => setConfirmUndo(imp)}>
                        Undo
                      </Button>
                    )}
                    {organised <= 0 && imp.status !== "organising" && (
                      <Button size="sm" variant="ghost" icon={<Trash2 size={13} />} onClick={() => void remove(imp)} aria-label="Remove from history">
                        Remove
                      </Button>
                    )}
                  </div>
                  {expanded && (
                    <div className="border-t border-line">
                      <dl className="grid grid-cols-2 gap-x-6 gap-y-2 px-4 py-3 text-xs" data-selectable>
                        <div>
                          <dt className="text-faint">Source folders</dt>
                          {imp.sources.map((s) => (
                            <dd key={s} className="break-all font-mono text-[11px] text-muted">{s}</dd>
                          ))}
                        </div>
                        <div>
                          <dt className="text-faint">Destination</dt>
                          <dd className="break-all font-mono text-[11px] text-muted">{imp.libraryRoot || imp.outputDir || "—"}</dd>
                          <dt className="mt-2 text-faint">Files discovered</dt>
                          <dd className="text-muted">
                            {formatNumber(imp.stats.filesDiscovered)} ({formatNumber(imp.stats.supported)} supported, {formatNumber(imp.stats.unsupported)} unsupported, {formatNumber(imp.stats.ignored)} system files)
                          </dd>
                        </div>
                      </dl>
                      {imp.messages.length > 0 && (
                        <ul className="grid gap-1 border-t border-line px-4 py-3 text-xs text-muted">
                          {imp.messages.slice(-30).map((m, i) => (
                            <li key={i}>{m}</li>
                          ))}
                        </ul>
                      )}
                      <OperationsList importId={imp.id} />
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <Modal
        open={!!confirmUndo}
        title="Undo this import?"
        onClose={() => setConfirmUndo(null)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirmUndo(null)}>Cancel</Button>
            <Button variant="danger" disabled={busy} onClick={() => confirmUndo && void undo(confirmUndo)}>
              Undo
            </Button>
          </>
        }
      >
        <div className="grid gap-2 text-[13px] text-muted">
          {confirmUndo?.mode === "move" ? (
            <p>Moved files will be returned to their original folders. A file is only moved back if its original location is still free.</p>
          ) : (
            <p>Copies created by this import will be deleted from the library. Your original sample packs are not touched.</p>
          )}
          <p>Files that were edited after organising are left in place and listed for you to review.</p>
        </div>
      </Modal>
      <Modal open={!!undoResult} title="Undo finished" onClose={() => setUndoResult(null)} footer={<Button variant="primary" onClick={() => setUndoResult(null)}>Close</Button>}>
        {undoResult && (
          <div className="grid gap-3 text-[13px]">
            <p>{formatNumber(undoResult.reverted)} file(s) reverted.</p>
            {undoResult.skipped.length > 0 && (
              <>
                <p className="text-warn">{undoResult.skipped.length} file(s) could not be safely reverted:</p>
                <ul className="max-h-60 divide-y divide-line overflow-auto rounded border border-line" data-selectable>
                  {undoResult.skipped.map((s, i) => (
                    <li key={i} className="px-2.5 py-1.5 text-[11px]">
                      <div className="truncate font-mono text-fg">{s.path}</div>
                      <div className="text-muted">{s.message}</div>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </div>
        )}
      </Modal>
    </div>
  );
}
