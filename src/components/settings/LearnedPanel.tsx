import { Brain, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useApp } from "../../hooks/useApp";
import { api, errorMessage } from "../../services/api";
import type { LearnedRule } from "../../types";
import { formatDate } from "../../utils/format";
import { Badge, Button, EmptyState, IconButton } from "../ui";

/** Rules learned from manual reassignments in the Review screen. */
export function LearnedPanel() {
  const { toast } = useApp();
  const [rules, setRules] = useState<LearnedRule[] | null>(null);
  const load = useCallback(() => {
    api
      .listLearnedRules()
      .then(setRules)
      .catch((e) => toast(errorMessage(e), "error"));
  }, [toast]);
  useEffect(load, [load]);

  const remove = async (id: number) => {
    try {
      await api.deleteLearnedRule(id);
      load();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };
  const clear = async () => {
    try {
      await api.clearLearnedRules();
      load();
      toast("All learned rules removed", "ok");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  return (
    <div className="grid gap-4">
      <div className="flex items-start justify-between gap-4">
        <p className="text-xs text-muted">
          Each time you change a sample's category in Review, its naming pattern is remembered. Matching files are then classified the same way —
          strongest within the same vendor or pack. Use <strong>Re-classify</strong> in Review to apply new rules to an import you are reviewing.
        </p>
        {rules && rules.length > 0 && (
          <Button size="sm" variant="danger" onClick={() => void clear()}>
            Forget all
          </Button>
        )}
      </div>
      {rules === null ? null : rules.length === 0 ? (
        <EmptyState icon={<Brain size={18} />} title="Nothing learned yet">
          Reassign a sample in the Review screen and its pattern will appear here.
        </EmptyState>
      ) : (
        <ul className="divide-y divide-line rounded-lg border border-line bg-panel" aria-label="Learned rules">
          {rules.map((r) => (
            <li key={r.id} className="flex items-center gap-3 px-3 py-2">
              <div className="min-w-0 flex-1">
                <div className="truncate text-[13px]">
                  {r.description} → <span className="text-accent">{r.categoryPath ?? `${r.categoryId} (deleted category)`}</span>
                </div>
                <div className="truncate text-[11px] text-faint" data-selectable>
                  from “{r.example}” · {formatDate(r.createdAt)}
                  {r.scope && <> · strongest in <span className="text-muted">{r.scope}</span></>}
                </div>
              </div>
              {r.hits > 1 && <Badge>×{r.hits}</Badge>}
              <IconButton label={`Forget rule for ${r.example}`} onClick={() => void remove(r.id)}>
                <Trash2 size={13} />
              </IconButton>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
