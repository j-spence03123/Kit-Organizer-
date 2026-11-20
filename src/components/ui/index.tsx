// Small, consistent UI primitives used across pages.
import { type ButtonHTMLAttributes, type ReactNode, useEffect, useRef } from "react";
import { X } from "lucide-react";
import { cx, percent } from "../../utils/format";

type Variant = "primary" | "secondary" | "ghost" | "danger";

export function Button({
  variant = "secondary",
  size = "md",
  icon,
  className,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: "sm" | "md"; icon?: ReactNode }) {
  const base =
    "inline-flex items-center justify-center gap-1.5 rounded-md font-medium transition-colors disabled:opacity-40 disabled:pointer-events-none whitespace-nowrap";
  const sizes = { sm: "h-7 px-2.5 text-xs", md: "h-8 px-3 text-[13px]" };
  const variants: Record<Variant, string> = {
    primary: "bg-accent text-black hover:bg-accent-strong",
    secondary: "bg-raised text-fg border border-line-strong hover:bg-[#2e2e34]",
    ghost: "text-muted hover:text-fg hover:bg-raised",
    danger: "bg-bad/15 text-bad border border-bad/30 hover:bg-bad/25",
  };
  return (
    <button type="button" className={cx(base, sizes[size], variants[variant], className)} {...rest}>
      {icon}
      {children}
    </button>
  );
}

export function IconButton({
  label,
  className,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={cx(
        "inline-flex h-7 w-7 items-center justify-center rounded-md text-muted transition-colors hover:bg-raised hover:text-fg disabled:opacity-40",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
}

export function Panel({ className, children, title, actions }: { className?: string; children: ReactNode; title?: ReactNode; actions?: ReactNode }) {
  return (
    <section className={cx("rounded-lg border border-line bg-panel", className)}>
      {(title || actions) && (
        <header className="flex items-center justify-between gap-3 border-b border-line px-4 py-2.5">
          <h2 className="text-[13px] font-semibold text-fg">{title}</h2>
          <div className="flex items-center gap-2">{actions}</div>
        </header>
      )}
      {children}
    </section>
  );
}

export function Badge({ tone = "neutral", children, className }: { tone?: "neutral" | "accent" | "ok" | "warn" | "bad"; children: ReactNode; className?: string }) {
  const tones = {
    neutral: "bg-raised text-muted border-line-strong",
    accent: "bg-accent-soft text-accent border-accent/30",
    ok: "bg-ok/10 text-ok border-ok/25",
    warn: "bg-warn/10 text-warn border-warn/25",
    bad: "bg-bad/10 text-bad border-bad/25",
  };
  return (
    <span className={cx("inline-flex items-center gap-1 rounded border px-1.5 py-px text-[11px] font-medium leading-4", tones[tone], className)}>
      {children}
    </span>
  );
}

export function ConfidenceBar({ value, threshold }: { value: number; threshold: number }) {
  const tone = value >= threshold ? (value >= 0.8 ? "bg-ok" : "bg-accent") : "bg-warn";
  return (
    <div className="flex items-center gap-2" title={`Rule-based confidence ${percent(value)}`}>
      <div className="h-1.5 w-14 overflow-hidden rounded-full bg-raised">
        <div className={cx("h-full rounded-full", tone)} style={{ width: `${Math.round(value * 100)}%` }} />
      </div>
      <span className="tabular w-8 text-right text-xs text-muted">{percent(value)}</span>
    </div>
  );
}

export function ProgressBar({ value, indeterminate }: { value?: number; indeterminate?: boolean }) {
  return (
    <div className="relative h-1.5 w-full overflow-hidden rounded-full bg-raised" role="progressbar" aria-valuenow={value != null ? Math.round(value * 100) : undefined}>
      {indeterminate ? (
        <div className="animate-indeterminate absolute inset-y-0 w-1/3 rounded-full bg-accent" />
      ) : (
        <div className="h-full rounded-full bg-accent transition-[width] duration-200" style={{ width: `${Math.min(100, (value ?? 0) * 100)}%` }} />
      )}
    </div>
  );
}

export function Modal({
  open,
  title,
  onClose,
  children,
  footer,
  width = "max-w-lg",
  dismissable = true,
}: {
  open: boolean;
  title: ReactNode;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  width?: string;
  dismissable?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && dismissable) onClose();
    };
    window.addEventListener("keydown", onKey);
    ref.current?.focus();
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose, dismissable]);
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6" onMouseDown={() => dismissable && onClose()}>
      <div
        ref={ref}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-label={typeof title === "string" ? title : undefined}
        className={cx("flex max-h-full w-full flex-col rounded-xl border border-line-strong bg-panel shadow-2xl outline-none", width)}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="flex items-center justify-between border-b border-line px-5 py-3">
          <h2 className="text-sm font-semibold">{title}</h2>
          {dismissable && (
            <IconButton label="Close" onClick={onClose}>
              <X size={15} />
            </IconButton>
          )}
        </header>
        <div className="min-h-0 overflow-auto px-5 py-4">{children}</div>
        {footer && <footer className="flex justify-end gap-2 border-t border-line px-5 py-3">{footer}</footer>}
      </div>
    </div>
  );
}

export function Toggle({ checked, onChange, label, description }: { checked: boolean; onChange: (v: boolean) => void; label: string; description?: string }) {
  return (
    <label className="flex cursor-pointer items-start justify-between gap-4">
      <span>
        <span className="block text-[13px] text-fg">{label}</span>
        {description && <span className="mt-0.5 block text-xs text-muted">{description}</span>}
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        onClick={() => onChange(!checked)}
        className={cx("relative mt-0.5 h-5 w-9 shrink-0 rounded-full transition-colors", checked ? "bg-accent" : "bg-line-strong")}
      >
        <span className={cx("absolute top-0.5 h-4 w-4 rounded-full bg-white transition-transform", checked ? "translate-x-4" : "translate-x-0.5")} />
      </button>
    </label>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  ariaLabel,
}: {
  value: T;
  options: { value: T; label: string; icon?: ReactNode }[];
  onChange: (v: T) => void;
  ariaLabel: string;
}) {
  return (
    <div role="radiogroup" aria-label={ariaLabel} className="inline-flex w-fit justify-self-start rounded-md border border-line-strong bg-panel-2 p-0.5">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          onClick={() => onChange(o.value)}
          className={cx(
            "inline-flex items-center gap-1.5 rounded px-3 py-1 text-xs font-medium transition-colors",
            value === o.value ? "bg-raised text-fg shadow-sm" : "text-muted hover:text-fg",
          )}
        >
          {o.icon}
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="grid gap-1.5">
      <span className="text-xs font-medium text-muted">{label}</span>
      {children}
      {hint && <span className="text-xs text-faint">{hint}</span>}
    </div>
  );
}

/** Input styling without a width, for inputs that set their own. */
export const inputBase =
  "h-8 rounded-md border border-line-strong bg-panel-2 px-2.5 text-[13px] text-fg placeholder:text-faint focus:border-accent focus:outline-none";
export const inputClass = `${inputBase} w-full`;

export function EmptyState({ icon, title, children, action }: { icon: ReactNode; title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 px-6 py-16 text-center">
      <div className="flex h-11 w-11 items-center justify-center rounded-full bg-raised text-muted">{icon}</div>
      <div>
        <p className="text-sm font-medium text-fg">{title}</p>
        {children && <p className="mt-1 max-w-sm text-xs text-muted">{children}</p>}
      </div>
      {action}
    </div>
  );
}

export function Stat({ label, value, sub, icon }: { label: string; value: ReactNode; sub?: ReactNode; icon?: ReactNode }) {
  return (
    <div className="rounded-lg border border-line bg-panel px-4 py-3.5">
      <div className="flex items-center justify-between text-xs text-muted">
        <span>{label}</span>
        <span className="text-faint">{icon}</span>
      </div>
      <div className="tabular mt-1.5 text-2xl font-semibold tracking-tight">{value}</div>
      {sub && <div className="mt-0.5 text-xs text-faint">{sub}</div>}
    </div>
  );
}
