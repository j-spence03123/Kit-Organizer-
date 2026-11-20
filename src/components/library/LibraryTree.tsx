import { ChevronDown, ChevronRight, Folder, FolderOpen } from "lucide-react";
import { useMemo, useState } from "react";
import { cx, formatNumber } from "../../utils/format";

export interface TreeNode {
  name: string;
  path: string;
  count: number;
  children: TreeNode[];
}

/** Build a nested tree from flat `category path → count` pairs, summing counts upward. */
export function buildTree(entries: [string, number][]): TreeNode[] {
  const root: TreeNode = { name: "", path: "", count: 0, children: [] };
  for (const [path, count] of entries) {
    let cur = root;
    cur.count += count;
    const parts = path.split("/").filter(Boolean);
    parts.forEach((part, i) => {
      const p = parts.slice(0, i + 1).join("/");
      let next = cur.children.find((c) => c.name === part);
      if (!next) {
        next = { name: part, path: p, count: 0, children: [] };
        cur.children.push(next);
      }
      next.count += count;
      cur = next;
    });
  }
  const sort = (n: TreeNode) => {
    n.children.sort((a, b) => a.name.localeCompare(b.name));
    n.children.forEach(sort);
  };
  sort(root);
  return root.children;
}

function Node({ node, depth, selected, onSelect }: { node: TreeNode; depth: number; selected: string; onSelect: (p: string) => void }) {
  const [open, setOpen] = useState(depth === 0 || selected.startsWith(node.path + "/"));
  const has = node.children.length > 0;
  const active = selected === node.path;
  return (
    <li>
      <div
        className={cx("flex h-7 items-center gap-1 rounded-md pr-2 text-[13px]", active ? "bg-accent-soft text-fg" : "text-muted hover:bg-panel-2 hover:text-fg")}
        style={{ paddingLeft: 6 + depth * 14 }}
      >
        <button type="button" aria-label={open ? `Collapse ${node.name}` : `Expand ${node.name}`} className={cx("flex h-5 w-4 items-center justify-center text-faint", !has && "invisible")} onClick={() => setOpen(!open)}>
          {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </button>
        <button type="button" className="flex min-w-0 flex-1 items-center gap-1.5 text-left" onClick={() => onSelect(node.path)} aria-current={active ? "true" : undefined}>
          {active || open ? <FolderOpen size={14} className="shrink-0 text-accent" /> : <Folder size={14} className="shrink-0" />}
          <span className="truncate">{node.name}</span>
        </button>
        <span className="tabular text-[11px] text-faint">{formatNumber(node.count)}</span>
      </div>
      {open && has && (
        <ul>
          {node.children.map((c) => (
            <Node key={c.path} node={c} depth={depth + 1} selected={selected} onSelect={onSelect} />
          ))}
        </ul>
      )}
    </li>
  );
}

export function LibraryTree({ entries, selected, onSelect, rootLabel }: { entries: [string, number][]; selected: string; onSelect: (p: string) => void; rootLabel: string }) {
  const tree = useMemo(() => buildTree(entries), [entries]);
  const total = entries.reduce((a, [, n]) => a + n, 0);
  return (
    <nav aria-label="Library folders">
      <button
        type="button"
        className={cx("mb-1 flex h-7 w-full items-center gap-2 rounded-md px-2 text-[13px]", selected === "" ? "bg-accent-soft text-fg" : "text-muted hover:bg-panel-2")}
        onClick={() => onSelect("")}
      >
        <FolderOpen size={14} className="text-accent" />
        <span className="flex-1 truncate text-left font-medium">{rootLabel}</span>
        <span className="tabular text-[11px] text-faint">{formatNumber(total)}</span>
      </button>
      <ul>
        {tree.map((n) => (
          <Node key={n.path} node={n} depth={0} selected={selected} onSelect={onSelect} />
        ))}
      </ul>
    </nav>
  );
}
