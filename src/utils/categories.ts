// Pure helpers for working with the category tree (no React, easy to test).
import type { CategoryConfig, CategoryNode } from "../types";

export interface FlatCategory {
  id: string;
  name: string;
  path: string;
  depth: number;
  parentId: string | null;
  node: CategoryNode;
}

export function flatten(nodes: CategoryNode[], prefix = "", depth = 0, parentId: string | null = null): FlatCategory[] {
  const out: FlatCategory[] = [];
  for (const n of nodes) {
    const path = prefix ? `${prefix}/${n.name}` : n.name;
    out.push({ id: n.id, name: n.name, path, depth, parentId, node: n });
    out.push(...flatten(n.children, path, depth + 1, n.id));
  }
  return out;
}

export function findNode(nodes: CategoryNode[], id: string): CategoryNode | null {
  for (const n of nodes) {
    if (n.id === id) return n;
    const f = findNode(n.children, id);
    if (f) return f;
  }
  return null;
}

/** Deep clone so edits never mutate state in place. */
export function cloneConfig(c: CategoryConfig): CategoryConfig {
  return structuredClone(c);
}

/** Apply `fn` to the children list that contains `id`. */
export function withSiblings(
  nodes: CategoryNode[],
  id: string,
  fn: (siblings: CategoryNode[], index: number) => void,
): boolean {
  const i = nodes.findIndex((n) => n.id === id);
  if (i >= 0) {
    fn(nodes, i);
    return true;
  }
  return nodes.some((n) => withSiblings(n.children, id, fn));
}

export function descendantIds(node: CategoryNode): string[] {
  return [node.id, ...node.children.flatMap(descendantIds)];
}

export function newId(): string {
  return `custom-${Math.random().toString(36).slice(2, 10)}${Date.now().toString(36).slice(-4)}`;
}

export const INVALID_NAME = /[\\/:*?"<>|]/;

export function validateName(name: string, siblings: CategoryNode[], selfId?: string): string | null {
  const t = name.trim();
  if (!t) return "Name cannot be empty";
  if (t !== name) return "Remove leading or trailing spaces";
  if (INVALID_NAME.test(t)) return 'Names cannot contain \\ / : * ? " < > |';
  if (t.endsWith(".")) return "Names cannot end with a period";
  if (siblings.some((s) => s.id !== selfId && s.name.toLowerCase() === t.toLowerCase()))
    return "A category with this name already exists here";
  return null;
}

/** Rules (loop targets/default children) in other categories that point at any of `ids`. */
export function referencesTo(config: CategoryConfig, ids: string[]): FlatCategory[] {
  const set = new Set(ids);
  return flatten(config.categories).filter(
    (c) => !set.has(c.id) && ((c.node.loopTarget && set.has(c.node.loopTarget)) || (c.node.defaultChild && set.has(c.node.defaultChild))),
  );
}

export function systemIds(config: CategoryConfig): string[] {
  return [config.midiCategory, config.presetCategory, config.uncategorisedCategory];
}

/** Clear references to removed ids, optionally redirecting them. */
export function redirectReferences(nodes: CategoryNode[], removed: Set<string>, to: string | null): void {
  for (const n of nodes) {
    if (n.loopTarget && removed.has(n.loopTarget)) n.loopTarget = to;
    if (n.defaultChild && removed.has(n.defaultChild)) n.defaultChild = null;
    redirectReferences(n.children, removed, to);
  }
}

/** Alias collisions: the same alias on more than one category. */
export function aliasConflicts(config: CategoryConfig): Map<string, string[]> {
  const seen = new Map<string, string[]>();
  for (const c of flatten(config.categories)) {
    for (const a of c.node.aliases) {
      const key = a.replace(/^~/, "").toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
      if (!key) continue;
      seen.set(key, [...(seen.get(key) ?? []), c.path]);
    }
  }
  return new Map([...seen].filter(([, v]) => v.length > 1));
}
