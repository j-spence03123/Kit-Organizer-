import { CheckCircle2, Info, X, XCircle } from "lucide-react";
import { useApp } from "../../hooks/useApp";
import { cx } from "../../utils/format";

export function Toaster() {
  const { toasts, dismissToast } = useApp();
  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-[60] grid w-96 gap-2" aria-live="polite">
      {toasts.map((t) => (
        <div
          key={t.id}
          role={t.tone === "error" ? "alert" : "status"}
          className={cx(
            "pointer-events-auto flex items-start gap-2.5 rounded-lg border bg-panel-2 px-3.5 py-2.5 text-[13px] shadow-xl",
            t.tone === "error" ? "border-bad/40" : t.tone === "ok" ? "border-ok/30" : "border-line-strong",
          )}
        >
          <span className="mt-0.5">
            {t.tone === "error" ? <XCircle size={15} className="text-bad" /> : t.tone === "ok" ? <CheckCircle2 size={15} className="text-ok" /> : <Info size={15} className="text-accent" />}
          </span>
          <span className="min-w-0 flex-1 break-words">{t.message}</span>
          <button type="button" aria-label="Dismiss" onClick={() => dismissToast(t.id)} className="text-faint hover:text-fg">
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
