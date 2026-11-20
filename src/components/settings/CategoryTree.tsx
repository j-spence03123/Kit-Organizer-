import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import type { CategoryNode } from "../../types";
import { cx } from "../../utils/format";

function Row({ node, depth, selected, onSelect, usage }: { node: CategoryNode; depth: number; selected: string | null; onSelect: (id: string) => void; usage: Map<string, number> }) {
  const [open, setOpen] = useState(false);
  const has = node.children.length > 0;
  const n = usage.get(node.id) ?? 0;
  return (
    <li>
      <div
        className={cx("flex h-7 items-center gap-1 rounded-md pr-2 text-[13px]", selected === node.id ? "bg-accent-soft text-fg" : "text-muted hover:bg-panel-2 hover:text-fg")}
        style={{ paddingLeft: 4 + depth * 14 }}
      >
        <button type="button" className={cx("flex h-5 w-4 items-center justify-center text-faint", !has && "invisible")} onClick={() => setOpen(!open)} aria-label={open ? `Collapse ${node.name}` : `Expand ${node.name}`}>
          {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        </button>
        <button type="button" className="min-w-0 flex-1 truncate text-left" onClick={() => onSelect(node.id)}>
          {node.name}
        </button>
        {n > 0 && <span className="tabular text-[10px] text-accent" title={`${n} pending sample(s)`}>{n}</span>}
        <span className="tabular text-[10px] text-faint" title="Aliases">{node.aliases.length || ""}</span>
      </div>
      {open && has && (
        <ul>
          {node.children.map((c) => (
            <Row key={c.id} node={c} depth={depth + 1} selected={selected} onSelect={onSelect} usage={usage} />
          ))}
        </ul>
      )}
    </li>
  );
}

export function CategoryTree({ nodes, selected, onSelect, usage }: { nodes: CategoryNode[]; selected: string | null; onSelect: (id: string) => void; usage: Map<string, number> }) {
  return (
    <ul aria-label="Category hierarchy">
      {nodes.map((n) => (
        <Row key={n.id} node={n} depth={0} selected={selected} onSelect={onSelect} usage={usage} />
      ))}
    </ul>
  );
}
