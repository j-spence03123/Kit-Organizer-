import { Library as LibraryIcon, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { SampleDetail } from "../components/classification/SampleDetail";
import { Pager, SampleTable } from "../components/classification/SampleTable";
import { PageHeader } from "../components/layout/PageHeader";
import { LibraryTree } from "../components/library/LibraryTree";
import { Button, EmptyState, inputBase } from "../components/ui";
import { useApp } from "../hooks/useApp";
import { usePlayer } from "../hooks/useAudioPlayer";
import { useDebounced, useSampleQuery } from "../hooks/useSampleQuery";
import { api, errorMessage } from "../services/api";
import type { SampleQuery, SampleRow } from "../types";
import { cx } from "../utils/format";

const PAGE = 200;

export default function Library() {
  const { config, settings, dataVersion, navigate, toast } = useApp();
  const [tree, setTree] = useState<[string, number][]>([]);
  const [category, setCategory] = useState("");
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState("filename");
  const [descending, setDescending] = useState(false);
  const [offset, setOffset] = useState(0);
  const [focused, setFocused] = useState<SampleRow | null>(null);
  const [version, setVersion] = useState(0);
  const debounced = useDebounced(search);
  const player = usePlayer();
  // Optional "play on select" preference.
  const focusRow = (r: SampleRow) => {
    setFocused(r);
    if (settings?.preview.autoplay && r.fileKind === "audio") {
      void player.play(r.status === "organised" && r.destinationPath ? r.destinationPath : r.originalPath);
    }
  };

  useEffect(() => {
    api.libraryTree().then(setTree).catch(() => undefined);
  }, [dataVersion, version]);
  useEffect(() => setOffset(0), [category, debounced, sort, descending]);

  const query: SampleQuery = useMemo(
    () => ({ statuses: ["organised"], category: category || null, search: debounced || null, sort, descending, offset, limit: PAGE }),
    [category, debounced, sort, descending, offset],
  );
  const { page, loading } = useSampleQuery(query, dataVersion + version);
  useEffect(() => {
    if (focused) {
      const fresh = page.rows.find((r) => r.id === focused.id);
      if (fresh && fresh !== focused) setFocused(fresh);
    }
  }, [page.rows, focused]);

  if (tree.length === 0) {
    return (
      <div className="flex h-full flex-col">
        <PageHeader title="Library" />
        <EmptyState icon={<LibraryIcon size={20} />} title="Your organised library is empty" action={<Button variant="primary" onClick={() => navigate("import")}>Import samples</Button>}>
          Once you organise an import, its samples appear here grouped by category.
        </EmptyState>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Library"
        subtitle="Browse organised samples by category."
        actions={
          <div className="relative">
            <Search size={13} className="absolute left-2.5 top-2.5 text-faint" />
            <input className={cx(inputBase, "w-64 pl-8")} placeholder="Search the library" value={search} onChange={(e) => setSearch(e.target.value)} aria-label="Search library" />
          </div>
        }
      />
      <div className="flex min-h-0 flex-1">
        <aside className="w-64 shrink-0 overflow-auto border-r border-line bg-panel p-2">
          <LibraryTree entries={tree} selected={category} onSelect={setCategory} rootLabel={config?.rootFolder ?? "Organised Samples"} />
        </aside>
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex items-center justify-between border-b border-line px-4 py-2">
            <span className="truncate text-xs text-muted">{category || "All categories"}</span>
            <Pager offset={offset} limit={PAGE} total={page.total} onChange={setOffset} />
          </div>
          <div className={cx("min-h-0 flex-1 overflow-auto", loading && "opacity-70")}>
            <SampleTable
              rows={page.rows}
              columns={["filename", "category", "pack", "destination"]}
              threshold={settings?.confidenceThreshold ?? 0.6}
              rootFolder={config?.rootFolder ?? "Organised Samples"}
              focusedId={focused?.id ?? null}
              onFocus={focusRow}
              onSelect={setFocused}
              sort={sort}
              descending={descending}
              onSort={(k) => {
                if (k === sort) setDescending(!descending);
                else setSort(k);
              }}
              compact={settings?.appearance.compactRows}
              empty={<p className="px-6 py-10 text-center text-xs text-muted">No samples here.</p>}
            />
          </div>
        </div>
        <aside className="w-[340px] shrink-0 overflow-auto border-l border-line bg-panel" aria-label="Sample details">
          {focused ? (
            <SampleDetail
              key={focused.id}
              row={focused}
              threshold={settings?.confidenceThreshold ?? 0.6}
              editable={false}
              onReassign={() => undefined}
              onAccept={() => undefined}
              onDuplicate={() => undefined}
              onSourcePack={(ids, p) =>
                api
                  .setSourcePack(ids, p)
                  .then(() => setVersion((v) => v + 1))
                  .catch((e) => toast(errorMessage(e), "error"))
              }
            />
          ) : (
            <EmptyState icon={<LibraryIcon size={18} />} title="Select a sample to preview" />
          )}
        </aside>
      </div>
    </div>
  );
}
