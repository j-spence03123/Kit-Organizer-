import { ArrowDown, ArrowUp, Download, FlaskConical, FolderPlus, GitMerge, Plus, RotateCcw, Save, SlidersHorizontal, Trash2, Upload } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { PageHeader } from "../components/layout/PageHeader";
import { AliasEditor } from "../components/settings/AliasEditor";
import { CategoryTree } from "../components/settings/CategoryTree";
import { RulesPanel } from "../components/settings/RulesPanel";
import { Badge, Button, EmptyState, Field, Modal, inputClass } from "../components/ui";
import { useApp } from "../hooks/useApp";
import { api, errorMessage } from "../services/api";
import { pickJsonFile, pickSavePath } from "../services/dialogs";
import type { CategoryConfig, CategoryNode, CategoryRemap, Classification, KindHint } from "../types";
import {
  aliasConflicts,
  cloneConfig,
  descendantIds,
  findNode,
  flatten,
  newId,
  redirectReferences,
  referencesTo,
  systemIds,
  validateName,
  withSiblings,
} from "../utils/categories";
import { percent } from "../utils/format";

type Pending = { kind: "delete" | "merge"; id: string } | { kind: "reset" } | null;

export default function Categories() {
  const { config, setConfig, toast, settings, updateSettings } = useApp();
  const [draft, setDraft] = useState<CategoryConfig | null>(null);
  const [remaps, setRemaps] = useState<CategoryRemap[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [usage, setUsage] = useState<Map<string, number>>(new Map());
  const [pending, setPending] = useState<Pending>(null);
  const [mergeTarget, setMergeTarget] = useState("");
  const [nameDraft, setNameDraft] = useState("");
  const [showRules, setShowRules] = useState(false);
  const [testPath, setTestPath] = useState("XLNT Pack/Bass/XLNT_Neuro_Growl_F.wav");
  const [testResult, setTestResult] = useState<Classification | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (config && !draft) setDraft(cloneConfig(config));
  }, [config, draft]);
  const loadUsage = useCallback(() => {
    api
      .categoryUsage()
      .then((u) => setUsage(new Map(u)))
      .catch(() => undefined);
  }, []);
  useEffect(loadUsage, [loadUsage]);

  const dirty = useMemo(() => !!draft && !!config && JSON.stringify(draft) !== JSON.stringify(config), [draft, config]);
  const flat = useMemo(() => (draft ? flatten(draft.categories) : []), [draft]);
  const conflicts = useMemo(() => (draft ? aliasConflicts(draft) : new Map<string, string[]>()), [draft]);
  const node = draft && selected ? findNode(draft.categories, selected) : null;
  const entry = flat.find((f) => f.id === selected);
  const system = draft ? systemIds(draft) : [];

  useEffect(() => setNameDraft(node?.name ?? ""), [node?.id, node?.name]);

  if (!draft) return null;

  const edit = (fn: (c: CategoryConfig) => void) => {
    const next = cloneConfig(draft);
    fn(next);
    setDraft(next);
  };
  const editNode = (fn: (n: CategoryNode) => void) =>
    edit((c) => {
      const n = selected ? findNode(c.categories, selected) : null;
      if (n) fn(n);
    });

  const usageOf = (n: CategoryNode) => descendantIds(n).reduce((a, id) => a + (usage.get(id) ?? 0), 0);

  const addCategory = (parentId: string | null) => {
    const id = newId();
    edit((c) => {
      const list = parentId ? findNode(c.categories, parentId)?.children : c.categories;
      if (!list) return;
      let name = "New Category";
      let i = 2;
      while (list.some((s) => s.name.toLowerCase() === name.toLowerCase())) name = `New Category ${i++}`;
      // Keep Uncategorised last at the top level.
      const at = parentId ? list.length : Math.max(0, list.findIndex((s) => s.id === c.uncategorisedCategory));
      list.splice(at < 0 ? list.length : at, 0, { id, name, aliases: [], priority: 0, kind: "any", children: [] });
    });
    setSelected(id);
  };

  const move = (dir: -1 | 1) =>
    edit((c) => {
      if (!selected) return;
      withSiblings(c.categories, selected, (sib, i) => {
        const j = i + dir;
        if (j < 0 || j >= sib.length) return;
        const [x] = sib.splice(i, 1);
        if (x) sib.splice(j, 0, x);
      });
    });

  const rename = () => {
    if (!node || nameDraft === node.name) return;
    let siblings: CategoryNode[] = [];
    withSiblings(draft.categories, node.id, (s) => (siblings = s));
    const err = validateName(nameDraft, siblings, node.id);
    if (err) {
      toast(err, "error");
      setNameDraft(node.name);
      return;
    }
    editNode((n) => (n.name = nameDraft));
  };

  const doDelete = (id: string) => {
    const target = findNode(draft.categories, id);
    if (!target) return;
    const removed = new Set(descendantIds(target));
    edit((c) => {
      withSiblings(c.categories, id, (sib, i) => sib.splice(i, 1));
      redirectReferences(c.categories, removed, null);
    });
    setRemaps((r) => [...r, ...[...removed].map((fromId) => ({ fromId, toId: draft.uncategorisedCategory }))]);
    setSelected(null);
    setPending(null);
  };

  const doMerge = (id: string, into: string) => {
    const src = findNode(draft.categories, id);
    const dst = findNode(draft.categories, into);
    if (!src || !dst) return;
    if (descendantIds(src).includes(into)) {
      toast("Cannot merge a category into one of its own subcategories", "error");
      return;
    }
    const clash = src.children.find((ch) => dst.children.some((d) => d.name.toLowerCase() === ch.name.toLowerCase()));
    if (clash) {
      toast(`Both categories contain “${clash.name}”. Rename one first.`, "error");
      return;
    }
    edit((c) => {
      const s = findNode(c.categories, id)!;
      const d = findNode(c.categories, into)!;
      d.aliases = [...new Set([...d.aliases, ...s.aliases])];
      d.children.push(...s.children);
      withSiblings(c.categories, id, (sib, i) => sib.splice(i, 1));
      redirectReferences(c.categories, new Set([id]), into);
    });
    setRemaps((r) => [...r, { fromId: id, toId: into }]);
    setSelected(into);
    setPending(null);
    setMergeTarget("");
  };

  const save = async () => {
    setSaving(true);
    try {
      await api.saveCategoryConfig(draft, remaps);
      setConfig(cloneConfig(draft));
      setRemaps([]);
      loadUsage();
      toast("Categories saved. Use “Re-classify” in Review to apply them to pending samples.", "ok");
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      setSaving(false);
    }
  };

  const reset = async () => {
    try {
      const c = await api.resetCategoryConfig();
      setConfig(c);
      setDraft(cloneConfig(c));
      setRemaps([]);
      setSelected(null);
      setPending(null);
      loadUsage();
      toast("Categories reset to defaults", "ok");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const importFile = async () => {
    const path = await pickJsonFile();
    if (!path) return;
    try {
      const imported = await api.importConfig(path);
      setDraft(cloneConfig(imported.categories));
      setSelected(null);
      if (imported.settings && settings) {
        await updateSettings(imported.settings);
        toast("Imported categories (unsaved) and settings (applied). Review, then Save.", "ok");
      } else {
        toast("Imported categories. Review them, then Save to apply.", "ok");
      }
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const exportFile = async () => {
    if (dirty) {
      toast("Save or discard your changes before exporting.", "error");
      return;
    }
    const path = await pickSavePath("kit-organizer-config.json");
    if (!path) return;
    try {
      await api.exportConfig(path, true);
      toast("Configuration exported", "ok");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const runTest = async () => {
    try {
      setTestResult(await api.classifyPreview(testPath));
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const pendingNode = pending && pending.kind !== "reset" ? findNode(draft.categories, pending.id) : null;
  const pendingRefs = pendingNode ? referencesTo(draft, descendantIds(pendingNode)) : [];
  const descendantsForDefault = node ? flatten(node.children, entry?.path ?? "") : [];

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Categories"
        subtitle="Edit the folder hierarchy and the keywords that drive classification."
        actions={
          <>
            <Button size="sm" variant="ghost" icon={<Upload size={13} />} onClick={importFile}>Import</Button>
            <Button size="sm" variant="ghost" icon={<Download size={13} />} onClick={exportFile}>Export</Button>
            <Button size="sm" variant="ghost" icon={<RotateCcw size={13} />} onClick={() => setPending({ kind: "reset" })}>Reset to defaults</Button>
            <Button size="sm" variant="ghost" disabled={!dirty} onClick={() => { setDraft(config ? cloneConfig(config) : null); setRemaps([]); }}>Discard</Button>
            <Button variant="primary" icon={<Save size={14} />} disabled={!dirty || saving} onClick={save}>Save changes</Button>
          </>
        }
      />
      <div className="flex min-h-0 flex-1">
        <aside className="flex w-72 shrink-0 flex-col border-r border-line bg-panel">
          <div className="flex items-center justify-between border-b border-line px-3 py-2">
            <span className="text-xs text-muted">{flat.length} categories</span>
            <div className="flex gap-1">
              <Button size="sm" variant="ghost" icon={<SlidersHorizontal size={13} />} onClick={() => { setShowRules(true); setSelected(null); }}>Rules</Button>
              <Button size="sm" variant="ghost" icon={<Plus size={13} />} onClick={() => addCategory(null)}>Add</Button>
            </div>
          </div>
          <div className="min-h-0 flex-1 overflow-auto p-2">
            <CategoryTree nodes={draft.categories} selected={selected} onSelect={(id) => { setSelected(id); setShowRules(false); }} usage={usage} />
          </div>
        </aside>

        <div className="min-w-0 flex-1 overflow-auto px-6 py-5">
          {showRules && !node ? (
            <div className="mx-auto max-w-2xl">
              <h2 className="mb-4 text-sm font-semibold">Classification rules</h2>
              <RulesPanel draft={draft} onChange={setDraft} />
            </div>
          ) : node && entry ? (
            <div className="mx-auto grid max-w-2xl gap-5">
              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <div className="truncate font-mono text-[11px] text-faint">{entry.path}</div>
                  <div className="mt-1 flex gap-1.5">
                    {system.includes(node.id) && <Badge tone="accent">system</Badge>}
                    {(usage.get(node.id) ?? 0) > 0 && <Badge>{usage.get(node.id)} pending samples</Badge>}
                  </div>
                </div>
                <div className="flex shrink-0 gap-1">
                  <Button size="sm" variant="ghost" icon={<ArrowUp size={13} />} onClick={() => move(-1)} aria-label="Move up" />
                  <Button size="sm" variant="ghost" icon={<ArrowDown size={13} />} onClick={() => move(1)} aria-label="Move down" />
                  <Button size="sm" icon={<FolderPlus size={13} />} onClick={() => addCategory(node.id)}>Subcategory</Button>
                  <Button size="sm" icon={<GitMerge size={13} />} disabled={system.includes(node.id)} onClick={() => setPending({ kind: "merge", id: node.id })}>Merge</Button>
                  <Button size="sm" variant="danger" icon={<Trash2 size={13} />} disabled={system.includes(node.id)} onClick={() => setPending({ kind: "delete", id: node.id })}>Delete</Button>
                </div>
              </div>

              <Field label="Name" hint="Becomes the folder name. Renaming is safe: rules refer to categories by ID, and pending samples follow the new path.">
                <input className={inputClass} value={nameDraft} onChange={(e) => setNameDraft(e.target.value)} onBlur={rename} onKeyDown={(e) => e.key === "Enter" && rename()} aria-label="Category name" />
              </Field>

              <Field label="Aliases" hint="Matched as whole words, ignoring case and punctuation (hi-hat = hihat = HiHat). Multi-word phrases beat single words. ~ marks weak evidence.">
                <AliasEditor aliases={node.aliases} onChange={(a) => editNode((n) => (n.aliases = a))} conflicts={conflicts} />
              </Field>

              <div className="grid grid-cols-2 gap-4">
                <Field label={`Priority (${node.priority > 0 ? "+" : ""}${node.priority})`} hint="Breaks ties between conflicting matches.">
                  <input type="range" min={-5} max={5} step={1} value={node.priority} onChange={(e) => editNode((n) => (n.priority = Number(e.target.value)))} aria-label="Priority" className="accent-[var(--accent)]" />
                </Field>
                <Field label="Intended for">
                  <select className={inputClass} value={node.kind} onChange={(e) => editNode((n) => (n.kind = e.target.value as KindHint))} aria-label="Sample kind">
                    <option value="any">Any sample</option>
                    <option value="oneShot">One-shots</option>
                    <option value="loop">Loops</option>
                  </select>
                </Field>
                <Field label="When the sample is a loop, send to" hint="e.g. Kicks → Drum Loops/Kick Loops">
                  <select className={inputClass} value={node.loopTarget ?? ""} onChange={(e) => editNode((n) => (n.loopTarget = e.target.value || null))} aria-label="Loop target">
                    <option value="">Keep in this category</option>
                    {flat.filter((f) => f.id !== node.id).map((f) => (
                      <option key={f.id} value={f.id}>{f.path}</option>
                    ))}
                  </select>
                </Field>
                <Field label="Default subcategory" hint="Used when only this category matched.">
                  <select className={inputClass} value={node.defaultChild ?? ""} onChange={(e) => editNode((n) => (n.defaultChild = e.target.value || null))} aria-label="Default subcategory" disabled={node.children.length === 0}>
                    <option value="">This folder</option>
                    {descendantsForDefault.map((f) => (
                      <option key={f.id} value={f.id}>{f.path.slice((entry.path.length ?? 0) + 1)}</option>
                    ))}
                  </select>
                </Field>
              </div>
            </div>
          ) : (
            <EmptyState icon={<SlidersHorizontal size={18} />} title="Select a category to edit">
              Or open <strong>Rules</strong> to tune scoring weights and loop/one-shot indicators.
            </EmptyState>
          )}

          <section className="mx-auto mt-8 max-w-2xl rounded-lg border border-line bg-panel p-4">
            <h3 className="mb-2 flex items-center gap-2 text-[13px] font-semibold">
              <FlaskConical size={14} className="text-accent" /> Test a filename
            </h3>
            <p className="mb-2 text-xs text-muted">Enter a path like <span className="font-mono">Pack/Folder/File.wav</span>. Uses the saved configuration{dirty ? " — save first to test your edits" : ""}.</p>
            <div className="flex gap-2">
              <input className={inputClass} value={testPath} onChange={(e) => setTestPath(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void runTest()} aria-label="Test path" />
              <Button onClick={runTest}>Classify</Button>
            </div>
            {testResult && (
              <div className="mt-3 grid gap-1.5 text-xs" data-selectable>
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium text-fg">{testResult.categoryPath}</span>
                  <Badge tone="accent">{percent(testResult.confidence)}</Badge>
                  <Badge>{testResult.sampleType}</Badge>
                  {testResult.key && <Badge>key {testResult.key}</Badge>}
                  {testResult.bpm && <Badge>{testResult.bpm} BPM</Badge>}
                </div>
                {testResult.alternatives.length > 0 && <div className="text-muted">Alternatives: {testResult.alternatives.map((a) => a.path).join(", ")}</div>}
                <ul className="grid gap-1">
                  {testResult.evidence.map((e, i) => (
                    <li key={i} className="rounded bg-panel-2 px-2 py-1 font-mono text-[11px] text-muted">{e}</li>
                  ))}
                </ul>
              </div>
            )}
          </section>
        </div>
      </div>

      <Modal
        open={pending?.kind === "delete"}
        title={`Delete “${pendingNode?.name}”?`}
        onClose={() => setPending(null)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setPending(null)}>Cancel</Button>
            <Button variant="danger" onClick={() => pending && pending.kind === "delete" && doDelete(pending.id)}>Delete category</Button>
          </>
        }
      >
        {pendingNode && (
          <ul className="grid list-disc gap-1.5 pl-5 text-[13px] text-muted">
            {pendingNode.children.length > 0 && <li>{descendantIds(pendingNode).length - 1} subcategories will also be deleted.</li>}
            {pendingNode.aliases.length > 0 && <li>{pendingNode.aliases.length} aliases will stop matching.</li>}
            {pendingRefs.length > 0 && <li>Rules in {pendingRefs.map((r) => r.path).join(", ")} point here and will be cleared.</li>}
            {usageOf(pendingNode) > 0 && <li>{usageOf(pendingNode)} pending sample(s) will move to Uncategorised.</li>}
            <li>Files already organised are not affected.</li>
          </ul>
        )}
      </Modal>

      <Modal
        open={pending?.kind === "merge"}
        title={`Merge “${pendingNode?.name}” into…`}
        onClose={() => setPending(null)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setPending(null)}>Cancel</Button>
            <Button variant="primary" disabled={!mergeTarget} onClick={() => pending && pending.kind === "merge" && doMerge(pending.id, mergeTarget)}>Merge</Button>
          </>
        }
      >
        {pendingNode && (
          <div className="grid gap-3 text-[13px]">
            <select className={inputClass} value={mergeTarget} onChange={(e) => setMergeTarget(e.target.value)} aria-label="Merge target">
              <option value="">Choose a category</option>
              {flat.filter((f) => !descendantIds(pendingNode).includes(f.id)).map((f) => (
                <option key={f.id} value={f.id}>{f.path}</option>
              ))}
            </select>
            <p className="text-xs text-muted">
              Aliases and subcategories move to the target, rules pointing here are redirected{usageOf(pendingNode) > 0 ? `, and ${usageOf(pendingNode)} pending sample(s) are reassigned` : ""}.
            </p>
          </div>
        )}
      </Modal>

      <Modal
        open={pending?.kind === "reset"}
        title="Reset categories to defaults?"
        onClose={() => setPending(null)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setPending(null)}>Cancel</Button>
            <Button variant="danger" onClick={reset}>Reset</Button>
          </>
        }
      >
        <p className="text-[13px] text-muted">All custom categories, aliases and weights will be replaced with the built-in hierarchy. Pending samples in custom categories move to Uncategorised. Consider exporting first.</p>
      </Modal>
    </div>
  );
}
