import { FolderInput, FolderTree, History, LayoutDashboard, Library, ListChecks, Settings } from "lucide-react";
import type { ReactNode } from "react";
import { type Page, useApp } from "../../hooks/useApp";
import { cx } from "../../utils/format";

const items: { page: Page; label: string; icon: ReactNode }[] = [
  { page: "dashboard", label: "Dashboard", icon: <LayoutDashboard size={16} /> },
  { page: "import", label: "Import Samples", icon: <FolderInput size={16} /> },
  { page: "review", label: "Review", icon: <ListChecks size={16} /> },
  { page: "library", label: "Library", icon: <Library size={16} /> },
  { page: "history", label: "History", icon: <History size={16} /> },
  { page: "categories", label: "Categories", icon: <FolderTree size={16} /> },
  { page: "settings", label: "Settings", icon: <Settings size={16} /> },
];

export function Logo() {
  return (
    <svg width="22" height="22" viewBox="0 0 24 24" aria-hidden="true">
      <rect x="1" y="1" width="22" height="22" rx="6" fill="#1c1c20" stroke="#3a3a42" />
      <rect x="6" y="7" width="12" height="2.6" rx="1" fill="var(--accent)" />
      <rect x="6" y="10.8" width="12" height="2.6" rx="1" fill="var(--accent)" opacity="0.8" />
      <rect x="6" y="14.6" width="12" height="2.6" rx="1" fill="var(--accent)" opacity="0.6" />
    </svg>
  );
}

export function Sidebar() {
  const { page, navigate } = useApp();
  return (
    <nav aria-label="Main" className="flex w-52 shrink-0 flex-col border-r border-line bg-panel">
      <div className="flex items-center gap-2.5 px-4 py-4">
        <Logo />
        <span className="text-[14px] font-semibold tracking-tight">Kit Organizer</span>
      </div>
      <ul className="grid gap-0.5 px-2">
        {items.map((it) => (
          <li key={it.page}>
            <button
              type="button"
              onClick={() => navigate(it.page)}
              aria-current={page === it.page ? "page" : undefined}
              className={cx(
                "flex w-full items-center gap-2.5 rounded-md px-2.5 py-1.5 text-[13px] transition-colors",
                page === it.page ? "bg-raised text-fg" : "text-muted hover:bg-panel-2 hover:text-fg",
              )}
            >
              <span className={page === it.page ? "text-accent" : undefined}>{it.icon}</span>
              {it.label}
            </button>
          </li>
        ))}
      </ul>
      <div className="mt-auto px-4 py-3 text-[11px] text-faint">Offline · local library</div>
    </nav>
  );
}
