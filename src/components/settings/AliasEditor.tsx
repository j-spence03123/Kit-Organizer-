import { X } from "lucide-react";
import { useState } from "react";
import { cx } from "../../utils/format";
import { inputClass } from "../ui";

/** Edits keyword aliases. A leading "~" marks an alias as weak evidence. */
export function AliasEditor({ aliases, onChange, conflicts }: { aliases: string[]; onChange: (a: string[]) => void; conflicts: Map<string, string[]> }) {
  const [text, setText] = useState("");
  const add = () => {
    const parts = text
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean)
      .filter((s) => !aliases.includes(s));
    if (parts.length) onChange([...aliases, ...parts]);
    setText("");
  };
  const keyOf = (a: string) => a.replace(/^~/, "").toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
  return (
    <div className="grid gap-2">
      <div className="flex flex-wrap gap-1.5">
        {aliases.length === 0 && <span className="text-xs text-faint">No aliases — this category is only reachable by manual assignment or as a default/loop target.</span>}
        {aliases.map((a, i) => {
          const weak = a.startsWith("~");
          const clash = (conflicts.get(keyOf(a))?.length ?? 0) > 1;
          return (
            <span
              key={a + i}
              className={cx("inline-flex items-center gap-1 rounded border py-0.5 pl-1.5 pr-1 text-[12px]", weak ? "border-dashed border-line-strong text-muted" : "border-line-strong text-fg", clash && "border-warn/60")}
              title={clash ? `Also used by: ${conflicts.get(keyOf(a))?.join(", ")}` : weak ? "Weak alias (half weight)" : undefined}
            >
              <button
                type="button"
                className={cx("rounded px-0.5 font-mono text-[10px]", weak ? "text-accent" : "text-faint hover:text-fg")}
                onClick={() => onChange(aliases.map((x, j) => (j === i ? (weak ? x.slice(1) : `~${x}`) : x)))}
                aria-label={weak ? `Make ${a} strong` : `Make ${a} weak`}
                title="Toggle weak"
              >
                ~
              </button>
              {a.replace(/^~/, "")}
              <button type="button" aria-label={`Remove alias ${a}`} className="text-faint hover:text-bad" onClick={() => onChange(aliases.filter((_, j) => j !== i))}>
                <X size={11} />
              </button>
            </span>
          );
        })}
      </div>
      <input
        className={inputClass}
        placeholder="Add aliases (comma separated, Enter to add)"
        aria-label="Add alias"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            add();
          }
        }}
        onBlur={add}
      />
    </div>
  );
}
