import { AlertTriangle, CheckCircle2, Copy, FolderOutput, Library, MoveRight, Undo2, XCircle } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useApp } from "../../hooks/useApp";
import { api, errorMessage, events } from "../../services/api";
import { pickFolders } from "../../services/dialogs";
import type { ExecReport, ImportRecord, OpMode, OrganiseProgress, PlanPreview, PlannedOp } from "../../types";
import { cx, formatBytes, formatNumber } from "../../utils/format";
import { Button, Modal, ProgressBar, Segmented } from "../ui";

type Step = "confirm" | "running" | "done";

function Row({ label, value, tone }: { label: string; value: string | number; tone?: "warn" | "bad" | "ok" }) {
  return (
    <div className="flex justify-between gap-3 py-1 text-[13px]">
      <span className="text-muted">{label}</span>
      <span className={cx("tabular", tone === "warn" ? "text-warn" : tone === "bad" ? "text-bad" : tone === "ok" ? "text-ok" : "text-fg")}>
        {typeof value === "number" ? formatNumber(value) : value}
      </span>
    </div>
  );
}

function OpList({ ops, show }: { ops: PlannedOp[]; show: "dest" | "reason" }) {
  if (ops.length === 0) return <p className="py-3 text-xs text-faint">None</p>;
  return (
    <ul className="max-h-48 divide-y divide-line overflow-auto rounded border border-line" data-selectable>
      {ops.map((o) => (
        <li key={o.sampleId} className="px-2.5 py-1.5 text-[11px]">
          <div className="truncate font-mono text-fg">{show === "dest" ? `${o.categoryPath}/${o.destName}` : o.src.split(/[\\/]/).pop()}</div>
          <div className="truncate text-faint">{show === "dest" ? (o.renamedFrom ? `renamed from ${o.renamedFrom}` : o.src) : o.reason}</div>
        </li>
      ))}
    </ul>
  );
}

export function OrganiseDialog({ open, onClose, imp, onFinished }: { open: boolean; onClose: () => void; imp: ImportRecord; onFinished: () => void }) {
  const { toast, navigate, settings } = useApp();
  const [step, setStep] = useState<Step>("confirm");
  const [output, setOutput] = useState(imp.outputDir || settings?.defaultOutputDir || "");
  const [mode, setMode] = useState<OpMode>(imp.mode);
  const [confirmMove, setConfirmMove] = useState(false);
  const [plan, setPlan] = useState<PlanPreview | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [tab, setTab] = useState<"transfers" | "renamed" | "rerouted" | "skipped">("transfers");
  const [progress, setProgress] = useState<OrganiseProgress | null>(null);
  const [report, setReport] = useState<ExecReport | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const [undoing, setUndoing] = useState(false);
  const jobRef = useRef<string | null>(null);

  const loadPlan = useCallback(async () => {
    if (!output) {
      setPlan(null);
      setPlanError("Choose an output folder to see the plan.");
      return;
    }
    setLoading(true);
    try {
      setPlan(await api.previewPlan(imp.id, output, mode));
      setPlanError(null);
    } catch (e) {
      setPlan(null);
      setPlanError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }, [imp.id, output, mode]);

  useEffect(() => {
    if (open && step === "confirm") void loadPlan();
  }, [open, step, loadPlan]);

  useEffect(() => {
    if (!open) return;
    const subs = [
      events.onOrganiseProgress((p) => {
        if (p.jobId === jobRef.current) setProgress(p);
      }),
      events.onOrganiseFinished((e) => {
        if (e.jobId !== jobRef.current) return;
        jobRef.current = null;
        if (e.ok && e.report) setReport(e.report);
        else setRunError(e.error ?? "Unknown error");
        setStep("done");
        onFinished();
      }),
    ];
    return () => subs.forEach((p) => void p.then((u) => u()));
  }, [open, onFinished]);

  const start = async () => {
    setRunError(null);
    setReport(null);
    setProgress(null);
    try {
      jobRef.current = imp.id;
      setStep("running");
      await api.organise(imp.id, output, mode, mode === "move" && confirmMove);
    } catch (e) {
      jobRef.current = null;
      setStep("confirm");
      toast(errorMessage(e), "error");
    }
  };

  const undo = async () => {
    setUndoing(true);
    try {
      const r = await api.undoImport(imp.id);
      toast(
        r.skipped.length ? `Undo reverted ${r.reverted} file(s); ${r.skipped.length} could not be safely reverted — see History.` : `Undo reverted ${r.reverted} file(s).`,
        r.skipped.length ? "info" : "ok",
      );
      onFinished();
      onClose();
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      setUndoing(false);
    }
  };

  const s = plan?.summary;
  const blocked = !plan || !s || s.undecidedDuplicates > 0 || s.transfers === 0 || (mode === "move" && !confirmMove);
  const spaceShort = plan && s && plan.availableBytes != null && mode === "copy" && s.bytes > plan.availableBytes;

  const footer =
    step === "confirm" ? (
      <>
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button variant="primary" onClick={start} disabled={blocked || loading || !!spaceShort}>
          {mode === "move" ? "Move" : "Copy"} {s ? formatNumber(s.transfers) : ""} files
        </Button>
      </>
    ) : step === "running" ? (
      <Button variant="ghost" onClick={() => void api.cancelJob(imp.id)}>
        Cancel
      </Button>
    ) : (
      <>
        {report && report.copied + report.moved > 0 && (
          <Button variant="ghost" icon={<Undo2 size={14} />} onClick={undo} disabled={undoing}>
            Undo this import
          </Button>
        )}
        <Button
          icon={<Library size={14} />}
          onClick={() => {
            onClose();
            navigate("library");
          }}
        >
          Open library
        </Button>
        <Button variant="primary" onClick={onClose}>
          Done
        </Button>
      </>
    );

  return (
    <Modal open={open} title={step === "done" ? "Organisation complete" : "Organise samples"} onClose={onClose} footer={footer} width="max-w-2xl" dismissable={step !== "running"}>
      {step === "confirm" && (
        <div className="grid gap-4">
          <div className="grid grid-cols-[1fr_auto] items-end gap-3">
            <div className="min-w-0">
              <div className="mb-1.5 text-xs text-muted">Output folder</div>
              <div className="flex h-8 items-center truncate rounded-md border border-line-strong bg-panel-2 px-2.5 font-mono text-[11px] text-muted" title={output} data-selectable>
                {output || "Not selected"}
              </div>
            </div>
            <Button
              icon={<FolderOutput size={14} />}
              onClick={async () => {
                const [p] = await pickFolders(false, "Choose output folder");
                if (p) setOutput(p);
              }}
            >
              Choose
            </Button>
          </div>
          <div className="flex items-center justify-between">
            <Segmented
              ariaLabel="Operation mode"
              value={mode}
              onChange={(m) => {
                setMode(m);
                setConfirmMove(false);
              }}
              options={[
                { value: "copy", label: "Copy (keep originals)", icon: <Copy size={13} /> },
                { value: "move", label: "Move", icon: <MoveRight size={13} /> },
              ]}
            />
            {plan && <span className="truncate pl-3 font-mono text-[11px] text-faint" title={plan.libraryRoot}>→ {plan.libraryRoot}</span>}
          </div>

          {loading && <ProgressBar indeterminate />}
          {planError && <p className="rounded-md border border-bad/30 bg-bad/10 p-2.5 text-xs text-bad">{planError}</p>}

          {s && plan && (
            <>
              <div className="grid grid-cols-2 gap-x-8 rounded-lg border border-line bg-panel-2 px-4 py-2">
                <Row label={mode === "move" ? "Files to move" : "Files to copy"} value={s.transfers} />
                <Row label="Total size" value={formatBytes(s.bytes)} />
                <Row label="Folders used" value={s.folders} />
                <Row label="Free space" value={plan.availableBytes != null ? formatBytes(plan.availableBytes) : "Unknown"} tone={spaceShort ? "bad" : undefined} />
                <Row label="Renamed to avoid collisions" value={s.renamed} tone={s.renamed ? "warn" : undefined} />
                <Row label="Duplicates skipped" value={s.skippedDuplicates} />
                <Row label="Duplicates kept" value={s.keptDuplicates} />
                <Row label="Low confidence → Uncategorised" value={s.lowConfidenceRerouted} tone={s.lowConfidenceRerouted ? "warn" : undefined} />
                <Row label="Low confidence left for later" value={s.lowConfidenceSkipped} />
                <Row label="Missing source files" value={s.missing} tone={s.missing ? "bad" : undefined} />
              </div>
              {s.undecidedDuplicates > 0 && (
                <p className="flex gap-2 rounded-md border border-warn/30 bg-warn/10 p-2.5 text-xs text-warn">
                  <AlertTriangle size={14} className="mt-px shrink-0" />
                  {formatNumber(s.undecidedDuplicates)} duplicate file(s) need a decision. Close this dialog, filter by “Duplicates”, and choose Keep or Skip.
                </p>
              )}
              {spaceShort && <p className="text-xs text-bad">Not enough free space on the destination drive.</p>}
              <div>
                <div className="mb-2 flex gap-1 text-xs">
                  {(
                    [
                      ["transfers", `Destinations (${Math.min(500, s.transfers)})`],
                      ["renamed", `Renamed (${s.renamed})`],
                      ["rerouted", `Rerouted (${plan.rerouted.length})`],
                      ["skipped", `Skipped (${plan.skipped.length})`],
                    ] as const
                  ).map(([k, l]) => (
                    <button key={k} type="button" onClick={() => setTab(k)} className={cx("rounded px-2 py-1", tab === k ? "bg-raised text-fg" : "text-muted hover:text-fg")}>
                      {l}
                    </button>
                  ))}
                </div>
                {tab === "transfers" && <OpList ops={plan.transfers} show="dest" />}
                {tab === "renamed" && <OpList ops={plan.renamed} show="dest" />}
                {tab === "rerouted" && <OpList ops={plan.rerouted} show="reason" />}
                {tab === "skipped" && <OpList ops={plan.skipped} show="reason" />}
              </div>
              {mode === "move" && (
                <label className="flex cursor-pointer gap-2.5 rounded-md border border-warn/40 bg-warn/10 p-3 text-xs text-warn">
                  <input type="checkbox" checked={confirmMove} onChange={(e) => setConfirmMove(e.target.checked)} className="mt-0.5 accent-[var(--color-warn)]" />
                  <span>
                    I understand that files will be <strong>moved out of their original sample packs</strong>. Each file is verified at the destination before the original is removed, and the operation can be undone from History.
                  </span>
                </label>
              )}
            </>
          )}
        </div>
      )}

      {step === "running" && (
        <div className="grid gap-3 py-2" aria-live="polite">
          <div className="flex justify-between text-[13px]">
            <span>{mode === "move" ? "Moving" : "Copying"} files…</span>
            <span className="tabular text-muted">
              {formatNumber(progress?.processed ?? 0)} / {formatNumber(progress?.total ?? s?.transfers ?? 0)}
            </span>
          </div>
          <ProgressBar value={progress && progress.bytesTotal > 0 ? progress.bytesDone / progress.bytesTotal : progress && progress.total ? progress.processed / progress.total : 0} />
          <div className="flex justify-between text-xs text-faint">
            <span className="truncate font-mono">{progress?.current}</span>
            <span className="tabular">
              {formatBytes(progress?.bytesDone ?? 0)} / {formatBytes(progress?.bytesTotal ?? s?.bytes ?? 0)}
            </span>
          </div>
          <p className="text-xs text-muted">Every copy is verified against its BLAKE3 hash. Cancelling stops after the current file; completed files stay in place and can be undone.</p>
        </div>
      )}

      {step === "done" && (
        <div className="grid gap-4">
          {runError ? (
            <div className="flex gap-3 rounded-lg border border-bad/30 bg-bad/10 p-3">
              <XCircle size={18} className="shrink-0 text-bad" />
              <div className="text-[13px]">
                <div className="font-medium text-bad">Organising could not start</div>
                <div className="mt-1 text-muted">{runError}</div>
                <div className="mt-1 text-xs text-faint">No files were changed.</div>
              </div>
            </div>
          ) : (
            report && (
              <>
                <div className="flex items-center gap-3">
                  {report.failed || report.cancelled ? <AlertTriangle size={22} className="text-warn" /> : <CheckCircle2 size={22} className="text-ok" />}
                  <div>
                    <div className="text-sm font-medium">
                      {report.cancelled ? "Cancelled — completed files were kept" : report.failed ? "Finished with some errors" : "Your samples are organised"}
                    </div>
                    <div className="font-mono text-[11px] text-faint" data-selectable>{report.libraryRoot}</div>
                  </div>
                </div>
                <div className="grid grid-cols-5 gap-2 text-center">
                  {(
                    [
                      ["Copied", report.copied],
                      ["Moved", report.moved],
                      ["Renamed", report.renamed],
                      ["Skipped", report.skipped],
                      ["Errors", report.failed],
                    ] as const
                  ).map(([l, v]) => (
                    <div key={l} className="rounded-lg border border-line bg-panel-2 py-2.5">
                      <div className={cx("tabular text-lg font-semibold", l === "Errors" && v > 0 ? "text-bad" : "text-fg")}>{formatNumber(v)}</div>
                      <div className="text-[11px] text-muted">{l}</div>
                    </div>
                  ))}
                </div>
                {report.errors.length > 0 && (
                  <div>
                    <div className="mb-1.5 text-xs text-muted">Problems</div>
                    <ul className="max-h-40 divide-y divide-line overflow-auto rounded border border-line" data-selectable>
                      {report.errors.map((er, i) => (
                        <li key={i} className="px-2.5 py-1.5 text-[11px]">
                          <div className="truncate font-mono text-fg">{er.path}</div>
                          <div className="text-bad">{er.message}</div>
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
                {report.warnings.length > 0 && (
                  <ul className="grid gap-1 text-xs text-warn">
                    {report.warnings.slice(0, 20).map((w, i) => (
                      <li key={i}>{w}</li>
                    ))}
                  </ul>
                )}
              </>
            )
          )}
        </div>
      )}
    </Modal>
  );
}
