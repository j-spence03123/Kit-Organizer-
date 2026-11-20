import { AlertTriangle, Copy, Folder, FolderOutput, MoveRight, Play, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { DropZone } from "../components/import/DropZone";
import { ScanProgressPanel } from "../components/import/ScanProgressPanel";
import { PageHeader } from "../components/layout/PageHeader";
import { Button, Field, IconButton, Panel, Segmented } from "../components/ui";
import { useApp } from "../hooks/useApp";
import { useNativeDrop } from "../hooks/useDragDrop";
import { api, errorMessage, events } from "../services/api";
import { pickFolders } from "../services/dialogs";
import type { OpMode, ScanProgress } from "../types";
import { basename, formatNumber } from "../utils/format";

export default function ImportPage() {
  const { settings, toast, navigate, setActiveImportId, bumpData, config } = useApp();
  const [queue, setQueue] = useState<string[]>([]);
  const [output, setOutput] = useState<string>("");
  const [mode, setMode] = useState<OpMode>("copy");
  const [jobId, setJobId] = useState<string | null>(null);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const jobRef = useRef<string | null>(null);

  useEffect(() => {
    if (settings) {
      setOutput((o) => o || settings.defaultOutputDir || "");
      setMode(settings.defaultMode);
    }
  }, [settings]);

  const addPaths = useCallback((paths: string[]) => {
    setQueue((q) => [...q, ...paths.filter((p) => !q.includes(p))]);
  }, []);
  const over = useNativeDrop(addPaths);

  useEffect(() => {
    const subs = [
      events.onScanProgress((p) => {
        if (p.jobId === jobRef.current) setProgress(p);
      }),
      events.onScanFinished((e) => {
        if (e.jobId !== jobRef.current) return;
        jobRef.current = null;
        setJobId(null);
        setCancelling(false);
        setProgress(null);
        if (e.ok) {
          const s = e.report;
          toast(
            `Scan complete: ${formatNumber(s?.supported ?? 0)} samples ready for review${s?.unsupported ? `, ${formatNumber(s.unsupported)} unsupported files left untouched` : ""}.`,
            "ok",
          );
          setQueue([]);
          setActiveImportId(e.jobId);
          bumpData();
          navigate("review");
        } else if (e.cancelled) {
          toast("Scan cancelled. No files were changed.");
        } else {
          toast(`Scan failed: ${e.error}`, "error");
        }
      }),
    ];
    return () => subs.forEach((p) => void p.then((u) => u()));
  }, [toast, navigate, setActiveImportId, bumpData]);

  const browse = async () => addPaths(await pickFolders(true, "Choose sample pack folders"));
  const browseOutput = async () => {
    const [p] = await pickFolders(false, "Choose output folder");
    if (p) setOutput(p);
  };

  const start = async () => {
    try {
      setProgress(null);
      const id = await api.startScan(queue, output || null, mode);
      jobRef.current = id;
      setJobId(id);
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const cancel = async () => {
    if (!jobId) return;
    setCancelling(true);
    await api.cancelJob(jobId);
  };

  const scanning = jobId !== null;
  return (
    <div className="flex h-full flex-col">
      <PageHeader title="Import Samples" subtitle="Add sample packs, choose where the organised library goes, then scan. You review everything before files are copied or moved." />
      <div className="min-h-0 flex-1 overflow-auto px-6 py-5">
        <div className="mx-auto grid max-w-5xl gap-5 lg:grid-cols-[1fr_340px]">
          <div className="grid content-start gap-4">
            <DropZone over={over} onBrowse={browse} disabled={scanning} />
            <Panel title={`Import queue (${queue.length})`} actions={queue.length > 0 && !scanning && <Button size="sm" variant="ghost" onClick={() => setQueue([])}>Clear</Button>}>
              {queue.length === 0 ? (
                <p className="px-4 py-6 text-center text-xs text-muted">No folders added yet.</p>
              ) : (
                <ul className="divide-y divide-line" aria-label="Import queue">
                  {queue.map((p) => (
                    <li key={p} className="flex items-center gap-3 px-4 py-2">
                      <Folder size={15} className="shrink-0 text-accent" />
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-[13px]">{basename(p)}</div>
                        <div className="truncate font-mono text-[11px] text-faint" data-selectable>{p}</div>
                      </div>
                      <IconButton label={`Remove ${basename(p)}`} onClick={() => setQueue((q) => q.filter((x) => x !== p))} disabled={scanning}>
                        <X size={14} />
                      </IconButton>
                    </li>
                  ))}
                </ul>
              )}
            </Panel>
            {scanning && <ScanProgressPanel progress={progress} onCancel={cancel} cancelling={cancelling} />}
          </div>

          <div className="grid content-start gap-4">
            <Panel title="Destination">
              <div className="grid gap-4 p-4">
                <Field label="Output folder" hint={output ? <>Library will be created at <span className="font-mono">{basename(output)}/{config?.rootFolder ?? "Organised Samples"}</span></> : "Required before organising (you can also choose it later)."}>
                  <div className="flex gap-2">
                    <div className="flex h-8 min-w-0 flex-1 items-center rounded-md border border-line-strong bg-panel-2 px-2.5 font-mono text-[11px] text-muted" title={output} data-selectable>
                      <span className="truncate">{output || "Not selected"}</span>
                    </div>
                    <Button icon={<FolderOutput size={14} />} onClick={browseOutput} disabled={scanning}>
                      Choose
                    </Button>
                  </div>
                </Field>
                <Field label="Operation">
                  <Segmented
                    ariaLabel="Operation mode"
                    value={mode}
                    onChange={setMode}
                    options={[
                      { value: "copy", label: "Copy", icon: <Copy size={13} /> },
                      { value: "move", label: "Move", icon: <MoveRight size={13} /> },
                    ]}
                  />
                </Field>
                {mode === "move" ? (
                  <p className="flex gap-2 rounded-md border border-warn/30 bg-warn/10 p-2.5 text-xs text-warn">
                    <AlertTriangle size={14} className="mt-px shrink-0" />
                    Move mode removes files from the original packs after each copy is verified. You will be asked to confirm before anything moves.
                  </p>
                ) : (
                  <p className="text-xs text-muted">Copy mode leaves your original sample packs untouched.</p>
                )}
              </div>
            </Panel>
            <Button variant="primary" icon={<Play size={14} />} disabled={queue.length === 0 || scanning} onClick={start} className="h-9">
              Scan {queue.length > 0 ? `${queue.length} folder${queue.length > 1 ? "s" : ""}` : ""}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
