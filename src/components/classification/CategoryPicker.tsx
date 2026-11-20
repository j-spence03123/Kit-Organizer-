import { ChevronDown, Search } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useApp } from "../../hooks/useApp";
import { cx } from "../../utils/format";
import { Button } from "../ui";

/** Searchable dropdown of every category in the hierarchy. */
export function CategoryPicker({
  onSelect,
  label = "Reassign to…",
  size = "sm",
  align = "left",
  disabled,
}: {
  onSelect: (id: string) => void;
  label?: string;
  size?: "sm" | "md";
  align?: "left" | "right";
  disabled?: boolean;
}) {
  const { categories } = useApp();
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);

  const filtered = useMemo(() => {
    const t = q.trim().toLowerCase();
    return t ? categories.filter((c) => c.path.toLowerCase().includes(t)) : categories;
  }, [categories, q]);

  return (
    <div className="relative" ref={ref}>
      <Button size={size} onClick={() => setOpen(!open)} disabled={disabled} aria-haspopup="listbox" aria-expanded={open}>
        {label}
        <ChevronDown size={13} />
      </Button>
      {open && (
        <div className={cx("absolute z-40 mt-1 w-80 rounded-lg border border-line-strong bg-panel-2 shadow-2xl", align === "right" ? "right-0" : "left-0")}>
          <div className="flex items-center gap-2 border-b border-line px-2.5">
            <Search size={13} className="text-faint" />
            <input
              autoFocus
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Search categories"
              aria-label="Search categories"
              className="h-8 flex-1 bg-transparent text-[13px] focus:outline-none"
              onKeyDown={(e) => {
                if (e.key === "Enter" && filtered[0]) {
                  onSelect(filtered[0].id);
                  setOpen(false);
                }
                if (e.key === "Escape") setOpen(false);
              }}
            />
          </div>
          <ul role="listbox" aria-label="Categories" className="max-h-80 overflow-auto py-1">
            {filtered.map((c) => (
              <li key={c.id}>
                <button
                  type="button"
                  role="option"
                  aria-selected={false}
                  onClick={() => {
                    onSelect(c.id);
                    setOpen(false);
                    setQ("");
                  }}
                  className="flex w-full items-center px-2.5 py-1 text-left text-[13px] hover:bg-raised"
                  style={{ paddingLeft: q ? 10 : 10 + c.depth * 14 }}
                >
                  <span className={c.depth === 0 ? "font-medium text-fg" : "text-muted"}>{q ? c.path : c.name}</span>
                </button>
              </li>
            ))}
            {filtered.length === 0 && <li className="px-3 py-2 text-xs text-faint">No matching categories</li>}
          </ul>
        </div>
      )}
    </div>
  );
}
