import { FolderOpen, UploadCloud } from "lucide-react";
import { cx } from "../../utils/format";
import { Button } from "../ui";

export function DropZone({ over, onBrowse, disabled }: { over: boolean; onBrowse: () => void; disabled?: boolean }) {
  return (
    <div
      data-testid="dropzone"
      className={cx(
        "flex flex-col items-center justify-center gap-3 rounded-xl border-2 border-dashed px-6 py-12 text-center transition-colors",
        over ? "border-accent bg-accent-soft" : "border-line-strong bg-panel-2/40",
        disabled && "opacity-50",
      )}
    >
      <div className={cx("flex h-12 w-12 items-center justify-center rounded-full", over ? "bg-accent text-black" : "bg-raised text-muted")}>
        <UploadCloud size={22} />
      </div>
      <div>
        <p className="text-sm font-medium">{over ? "Drop to add to the import queue" : "Drag sample pack folders here"}</p>
        <p className="mt-1 text-xs text-muted">Add as many packs as you like. Nothing is changed until you confirm the plan.</p>
      </div>
      <Button icon={<FolderOpen size={14} />} onClick={onBrowse} disabled={disabled}>
        Browse folders
      </Button>
    </div>
  );
}
